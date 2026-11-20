//! Finding asset files (ARCHITECTURE §16) and batch relinking.
//!
//! [`AssetLocator`] extends `bae_model::AssetResolver` (relative → original →
//! path mappings → search by name and fingerprint → missing) with assets embedded
//! in the `.baez` the project was opened from, which win over everything else.
//!
//! [`plan_relink`]: after the user finds one missing asset, the directory change
//! it implies ([`bae_model::asset::infer_mapping`]) and the folder it was found in
//! are used to find the other missing assets too (AE, Premiere and Blender
//! behave this way).

use std::fs;
use std::path::{Path, PathBuf};

use bae_model::asset::{hash_file, infer_mapping, to_slash};
use bae_model::{AssetFile, AssetResolver, PathMapping, PrimPath, Project, Resolution, Stage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::collect::{AssetEntry, AssetKind, CollectOptions, collect};
use crate::container::{ASSETS_DIR, PackageReader};
use crate::error::PackageError;
use crate::sequence;

/// How an asset file was found on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FoundVia {
    /// `file:relative` against the project's base directory.
    Relative,
    /// `file:original`.
    Original,
    /// `file:original` rewritten by a path mapping.
    Mapped,
    /// Same name and fingerprint in a search directory.
    Searched,
}

/// Where an asset's file is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetLocation {
    /// A file on disk.
    File {
        path: PathBuf,
        via: FoundVia,
    },
    /// An entry of the `.baez` the project was opened from.
    Packaged {
        entry: String,
    },
    Missing,
}

impl AssetLocation {
    pub fn is_missing(&self) -> bool {
        matches!(self, AssetLocation::Missing)
    }

    /// The file on disk, if the asset is on disk.
    pub fn path(&self) -> Option<&Path> {
        match self {
            AssetLocation::File { path, .. } => Some(path),
            _ => None,
        }
    }

    /// Where it is, for reports: the disk path or `<package>!/<entry>`.
    pub fn describe(&self, package: Option<&Path>) -> Option<String> {
        match self {
            AssetLocation::File { path, .. } => Some(to_slash(path)),
            AssetLocation::Packaged { entry } => Some(match package {
                Some(p) => format!("{}!/{entry}", to_slash(p)),
                None => entry.clone(),
            }),
            AssetLocation::Missing => None,
        }
    }

    fn from_resolution(r: Resolution) -> Self {
        match r {
            Resolution::Relative(path) => AssetLocation::File {
                path,
                via: FoundVia::Relative,
            },
            Resolution::Original(path) => AssetLocation::File {
                path,
                via: FoundVia::Original,
            },
            Resolution::Mapped(path) => AssetLocation::File {
                path,
                via: FoundVia::Mapped,
            },
            Resolution::Searched(path) => AssetLocation::File {
                path,
                via: FoundVia::Searched,
            },
            Resolution::Missing => AssetLocation::Missing,
        }
    }
}

/// One file of an asset (a sequence has one per frame).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileSource {
    Disk(PathBuf),
    /// Entry of the source package ([`AssetLocator::package`]).
    Packaged(String),
}

impl FileSource {
    /// File name (last path segment).
    pub fn file_name(&self) -> String {
        match self {
            FileSource::Disk(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            FileSource::Packaged(e) => e.rsplit('/').next().unwrap_or(e).to_owned(),
        }
    }
}

/// Finds asset files for one project.
#[derive(Clone, Debug, Default)]
pub struct AssetLocator {
    pub resolver: AssetResolver,
    package: Option<PackageReader>,
}

impl AssetLocator {
    /// A locator for files relative to `base_dir`, with path mappings.
    pub fn new(base_dir: Option<PathBuf>, mappings: Vec<PathMapping>) -> Self {
        AssetLocator {
            resolver: AssetResolver::new(base_dir, mappings),
            package: None,
        }
    }

    /// The locator for an open project: its base directory and path mappings,
    /// and its embedded assets when it was opened from a `.baez`.
    pub fn for_project(project: &Project) -> Result<Self, PackageError> {
        Self::for_location(project.location(), project.manifest.path_mappings.clone())
    }

    /// The locator for a project at `location` (`.bae` directory or `.baez`
    /// file; `None` for an unsaved project).
    pub fn for_location(
        location: Option<&Path>,
        mappings: Vec<PathMapping>,
    ) -> Result<Self, PackageError> {
        let is_baez = location.is_some_and(is_baez);
        let base_dir = location.and_then(|l| if is_baez { l.parent() } else { Some(l) });
        let package = match location {
            Some(l) if is_baez => Some(PackageReader::open(l)?),
            _ => None,
        };
        Ok(AssetLocator {
            resolver: AssetResolver::new(base_dir.map(Path::to_owned), mappings),
            package,
        })
    }

    /// Also search these directories by name and fingerprint.
    pub fn with_search_dirs(mut self, dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        self.resolver.search_dirs.extend(dirs);
        self
    }

    /// The package the project was opened from, if any.
    pub fn package(&self) -> Option<&PackageReader> {
        self.package.as_ref()
    }

    pub fn base_dir(&self) -> Option<&Path> {
        self.resolver.base_dir.as_deref()
    }

    /// Find an asset's file.
    pub fn locate(&self, file: &AssetFile) -> AssetLocation {
        if let (Some(pkg), Some(rel)) = (&self.package, &file.relative)
            && rel.starts_with(ASSETS_DIR)
            && pkg.contains(rel)
        {
            return AssetLocation::Packaged { entry: rel.clone() };
        }
        AssetLocation::from_resolution(self.resolver.resolve(file))
    }

    /// Every file an asset needs at `location`: the file itself, or all frames
    /// of an image sequence.
    pub fn files(
        &self,
        asset: &AssetEntry,
        location: &AssetLocation,
    ) -> Result<Vec<FileSource>, PackageError> {
        let pattern = asset.sequence.as_ref().and_then(|s| s.pattern.as_deref());
        let is_sequence = asset.kind == AssetKind::Sequence;
        Ok(match location {
            AssetLocation::Missing => Vec::new(),
            AssetLocation::File { path, .. } if is_sequence => {
                sequence::members_on_disk(path, pattern)
                    .map_err(crate::error::io_err(path))?
                    .into_iter()
                    .map(FileSource::Disk)
                    .collect()
            }
            AssetLocation::File { path, .. } => vec![FileSource::Disk(path.clone())],
            AssetLocation::Packaged { entry } if is_sequence => {
                let pkg = self
                    .package
                    .as_ref()
                    .expect("packaged location has a package");
                sequence::members_in(entry, pattern, pkg.entries().map(|e| e.name.as_str()))
                    .into_iter()
                    .map(FileSource::Packaged)
                    .collect()
            }
            AssetLocation::Packaged { entry } => vec![FileSource::Packaged(entry.clone())],
        })
    }

    /// Size of a file source in bytes.
    pub fn size(&self, source: &FileSource) -> u64 {
        match source {
            FileSource::Disk(p) => fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            FileSource::Packaged(e) => self
                .package
                .as_ref()
                .and_then(|p| p.entry(e))
                .map_or(0, |e| e.size),
        }
    }
}

pub(crate) fn is_baez(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(bae_model::project::BAEZ_EXTENSION))
}

/// A new file reference for an asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Relink {
    /// The `Asset` prim.
    pub path: PrimPath,
    /// Its new file reference.
    pub file: AssetFile,
}

/// Result of [`plan_relink`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RelinkPlan {
    /// The asset the user relinked first, then every other missing asset found
    /// through it.
    pub relinks: Vec<Relink>,
    /// The directory change the first relink implies (e.g. `Z:/` → `/Volumes/NAS/`);
    /// worth adding to the project's path mappings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapping: Option<PathMapping>,
    /// Assets that are still missing afterwards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub still_missing: Vec<PrimPath>,
}

/// Relink `asset` to `found` and, with `find_others`, look for the other missing
/// assets through the same directory change and in the same folder. Files are
/// matched by name and fingerprint (size, and the content hash when recorded).
pub fn plan_relink(
    stage: &Stage,
    locator: &AssetLocator,
    asset: &PrimPath,
    found: &Path,
    find_others: bool,
) -> Result<RelinkPlan, PackageError> {
    let view = stage
        .prim(asset)
        .filter(|v| v.is_a(&bae_model::PrimType::ASSET))
        .ok_or_else(|| PackageError::NotAnAsset(asset.clone()))?;
    let old = AssetFile::from_prim(&view);
    let with_hash = old.as_ref().is_some_and(|f| f.hash.is_some());
    let file = AssetFile::from_path(found, locator.base_dir(), with_hash)
        .map_err(crate::error::io_err(found))?;
    let mapping = old
        .as_ref()
        .and_then(|o| infer_mapping(&o.original, Path::new(&file.original)));
    let found_dir = Path::new(&file.original).parent().map(Path::to_owned);
    let mut plan = RelinkPlan {
        relinks: vec![Relink {
            path: asset.clone(),
            file,
        }],
        mapping: mapping.clone(),
        still_missing: Vec::new(),
    };
    if !find_others {
        return Ok(plan);
    }
    for other in collect(stage, &CollectOptions::default()).assets {
        let Some(file) = other.file.as_ref() else {
            continue;
        };
        if &other.path == asset || !locator.locate(file).is_missing() {
            continue;
        }
        let mapped = mapping
            .as_ref()
            .and_then(|m| m.apply(&file.original))
            .map(PathBuf::from);
        let sibling = found_dir.as_ref().map(|d| d.join(file.file_name()));
        let hit = mapped
            .into_iter()
            .chain(sibling)
            .find(|candidate| fingerprint_matches(candidate, file));
        match hit {
            Some(path) => plan.relinks.push(Relink {
                path: other.path.clone(),
                file: AssetFile::from_path(&path, locator.base_dir(), file.hash.is_some())
                    .map_err(crate::error::io_err(&path))?,
            }),
            None => plan.still_missing.push(other.path.clone()),
        }
    }
    Ok(plan)
}

/// Size must match; the content hash too when recorded.
fn fingerprint_matches(path: &Path, file: &AssetFile) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() != file.size {
        return false;
    }
    match &file.hash {
        Some(h) => hash_file(path).is_ok_and(|x| &x == h),
        None => true,
    }
}
