//! Packing a project with its assets (ARCHITECTURE §16):
//!
//! - [`pack`] writes a single-file `.baez`: project data deflated, every asset
//!   file stored uncompressed under `assets/` (zip64 for large media);
//! - [`collect_files`] writes the same layout as a plain `.bae` folder (AE
//!   *Collect Files*);
//! - [`scan`] reports what either would do without writing anything;
//! - [`unpack`] turns a `.baez` back into a `.bae` folder.
//!
//! The source project and its asset files are only read. The package gets a copy
//! of the project whose asset references point into the package
//! (`file:relative = assets/…`, through the `asset.set_files` command); the
//! original path, size, modification time and hash stay as provenance.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bae_model::asset::to_slash;
use bae_model::{
    AssetFile, AuthorId, Command as _, CommandRegistry, OpenOptions, PrimPath, Project,
    ProjectManifest, Stage, Sublayer, Time,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::collect::{AssetEntry, AssetKind, CollectOptions, Inventory, collect};
use crate::commands::SetAssetFiles;
use crate::container::ASSETS_DIR;
use crate::error::{PackageError, io_err};
use crate::report::{
    AssetReport, AssetStatus, FontReport, PackageReport, PackageStats, ReportKind, Warning,
    WarningCode,
};
use crate::resolve::{AssetLocation, AssetLocator, FileSource, Relink, is_baez};
use crate::sequence;

/// Key of the packaging note in a packaged project's `project.json`.
pub const MANIFEST_KEY: &str = "package";

/// The project to pack.
#[derive(Clone, Copy, Debug)]
pub struct PackSource<'a> {
    pub stage: &'a Stage,
    /// Manifest to carry over (path mappings, unknown fields); `None`: a default one.
    pub manifest: Option<&'a ProjectManifest>,
    /// Where the project is (`.bae` directory or `.baez` file); relative asset
    /// paths resolve against it, embedded assets of a `.baez` are read from it.
    /// `None` for an unsaved project.
    pub location: Option<&'a Path>,
}

impl<'a> PackSource<'a> {
    pub fn project(project: &'a Project) -> Self {
        PackSource {
            stage: &project.stage,
            manifest: Some(&project.manifest),
            location: project.location(),
        }
    }

    pub fn stage(stage: &'a Stage, location: Option<&'a Path>) -> Self {
        PackSource {
            stage,
            manifest: None,
            location,
        }
    }
}

/// Fonts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontPolicy {
    /// Record font names only; the receiver needs them installed (or gets a
    /// fallback font).
    #[default]
    Record,
    /// Pack the font files too (mind the font licenses). Reserved: not available
    /// yet, fonts are recorded.
    Embed,
}

/// How trimmed clips are produced (mirrors `bae_media::TrimMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TrimMode {
    /// Copy compressed samples on GOP boundaries (widening to the nearest
    /// keyframes) when possible, otherwise re-encode.
    #[default]
    PreferPassthrough,
    /// Always copy compressed samples.
    Passthrough,
    /// Re-encode exactly the used frames.
    Reencode,
}

/// Video trimming: pack only the source ranges the timeline uses. Reserved —
/// the parameters are fixed here, their defaults may still change; this version
/// packs whole files and reports that trimming was not done.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TrimOptions {
    /// Extra source time kept before and after each used range.
    #[serde(default)]
    pub handles: Time,
    #[serde(default)]
    pub mode: TrimMode,
    /// Codec when re-encoding (e.g. `prores_422_hq`); `None`: an intermediate
    /// codec chosen by the trimmer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
}

/// Packing options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackOptions {
    /// Pack for these comps (and what they use, recursively); empty: the whole
    /// project.
    #[serde(default)]
    pub roots: Vec<PrimPath>,
    /// Also pack assets nothing in scope uses. Default true; when false they keep
    /// their outside reference.
    #[serde(default = "yes")]
    pub include_unused: bool,
    /// Fail instead of packing without missing assets.
    #[serde(default)]
    pub fail_on_missing: bool,
    /// Extra directories to search for missing files by name and fingerprint.
    #[serde(default)]
    pub search_dirs: Vec<PathBuf>,
    /// Path mappings used in addition to the project's.
    #[serde(default)]
    pub path_mappings: Vec<bae_model::PathMapping>,
    #[serde(default)]
    pub fonts: FontPolicy,
    /// Video trimming (reserved, see [`TrimOptions`]). `None`: whole files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim: Option<TrimOptions>,
}

fn yes() -> bool {
    true
}

impl Default for PackOptions {
    fn default() -> Self {
        PackOptions {
            roots: Vec::new(),
            include_unused: true,
            fail_on_missing: false,
            search_dirs: Vec::new(),
            path_mappings: Vec::new(),
            fonts: FontPolicy::Record,
            trim: None,
        }
    }
}

/// A file going into the package.
#[derive(Clone, Debug)]
struct PlannedFile {
    /// Entry / relative path, `assets/…`.
    name: String,
    source: FileSource,
    size: u64,
}

#[derive(Debug)]
struct PlanItem {
    entry: AssetEntry,
    status: AssetStatus,
    location: AssetLocation,
    packaged_as: Option<String>,
    files: Vec<usize>,
}

#[derive(Debug)]
struct Plan {
    locator: AssetLocator,
    inventory: Inventory,
    items: Vec<PlanItem>,
    files: Vec<PlannedFile>,
    warnings: Vec<Warning>,
}

impl Plan {
    fn build(source: &PackSource<'_>, options: &PackOptions) -> Result<Self, PackageError> {
        let mut mappings = source
            .manifest
            .map(|m| m.path_mappings.clone())
            .unwrap_or_default();
        mappings.extend(options.path_mappings.iter().cloned());
        let locator = AssetLocator::for_location(source.location, mappings)?
            .with_search_dirs(options.search_dirs.iter().cloned());
        let inventory = collect(
            source.stage,
            &CollectOptions {
                roots: options.roots.clone(),
            },
        );

        let mut names = Names::default();
        let mut files: Vec<PlannedFile> = Vec::new();
        let mut items = Vec::with_capacity(inventory.assets.len());
        for entry in &inventory.assets {
            let mut item = PlanItem {
                entry: entry.clone(),
                status: AssetStatus::NoFile,
                location: AssetLocation::Missing,
                packaged_as: None,
                files: Vec::new(),
            };
            let Some(file) = &entry.file else {
                items.push(item);
                continue;
            };
            if !entry.is_used() && !options.include_unused {
                item.status = AssetStatus::Excluded;
                items.push(item);
                continue;
            }
            item.location = locator.locate(file);
            let sources = locator.files(entry, &item.location)?;
            if sources.is_empty() {
                item.status = AssetStatus::Missing;
                items.push(item);
                continue;
            }
            item.status = AssetStatus::Packed;
            let referenced = match &item.location {
                AssetLocation::File { path, .. } => FileSource::Disk(path.clone()).file_name(),
                AssetLocation::Packaged { entry } => {
                    FileSource::Packaged(entry.clone()).file_name()
                }
                AssetLocation::Missing => unreachable!("sources are empty when missing"),
            };
            let placed = if entry.kind == AssetKind::Sequence {
                names.place_sequence(&referenced, &sources)
            } else {
                vec![names.place_file(&sources[0])]
            };
            for (src, (name, fresh)) in sources.iter().zip(&placed) {
                let index = if *fresh {
                    files.push(PlannedFile {
                        name: name.clone(),
                        source: src.clone(),
                        size: locator.size(src),
                    });
                    files.len() - 1
                } else {
                    files
                        .iter()
                        .position(|f| &f.name == name)
                        .expect("placed before")
                };
                item.files.push(index);
            }
            item.packaged_as = Some(if entry.kind == AssetKind::Sequence {
                let first = &placed[0].0;
                let dir = first.rsplit_once('/').map_or(first.as_str(), |(d, _)| d);
                format!("{dir}/{}", sanitize(&referenced))
            } else {
                placed[0].0.clone()
            });
            items.push(item);
        }

        let mut plan = Plan {
            locator,
            inventory,
            items,
            files,
            warnings: Vec::new(),
        };
        plan.warn(options);
        let missing: Vec<PrimPath> = plan
            .items
            .iter()
            .filter(|i| i.status == AssetStatus::Missing)
            .map(|i| i.entry.path.clone())
            .collect();
        if options.fail_on_missing && !missing.is_empty() {
            return Err(PackageError::MissingAssets(missing));
        }
        Ok(plan)
    }

    fn warn(&mut self, options: &PackOptions) {
        let missing = self
            .items
            .iter()
            .filter(|i| i.status == AssetStatus::Missing)
            .count();
        if missing > 0 {
            self.warnings.push(Warning::new(
                WarningCode::MissingAssets,
                format!("{missing} asset file(s) could not be found and are not in the package"),
            ));
        }
        if !self.inventory.fonts.is_empty() {
            let message = match options.fonts {
                FontPolicy::Record => {
                    "fonts are recorded by name only; the receiver needs them installed"
                }
                FontPolicy::Embed => {
                    "embedding fonts is not available yet; fonts are recorded by name only"
                }
            };
            self.warnings
                .push(Warning::new(WarningCode::FontsNotEmbedded, message));
        }
        if !self.inventory.external_projects.is_empty() {
            self.warnings.push(Warning::new(
                WarningCode::ExternalProjects,
                format!(
                    "referenced projects are not packed: {}",
                    self.inventory.external_projects.join(", ")
                ),
            ));
        }
        if options.trim.is_some() {
            self.warnings.push(Warning::new(
                WarningCode::TrimNotAvailable,
                "clip trimming is not available yet; whole files were packed",
            ));
        }
    }

    /// The new references of packed assets.
    fn relinks(&self) -> Vec<Relink> {
        self.items
            .iter()
            .filter_map(|i| {
                let relative = i.packaged_as.clone()?;
                let file = i.entry.file.clone()?;
                Some(Relink {
                    path: i.entry.path.clone(),
                    file: AssetFile {
                        relative: Some(relative),
                        ..file
                    },
                })
            })
            .collect()
    }

    /// Files on disk that must not be overwritten by the output.
    fn disk_sources(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().filter_map(|f| match &f.source {
            FileSource::Disk(p) => Some(p.as_path()),
            FileSource::Packaged(_) => None,
        })
    }

    fn report(&self, kind: ReportKind, output: Option<&Path>, project_bytes: u64) -> PackageReport {
        let package = self.locator.package().map(|p| p.path());
        let assets: Vec<AssetReport> = self
            .items
            .iter()
            .map(|i| AssetReport {
                path: i.entry.path.clone(),
                name: i.entry.name.clone(),
                kind: i.entry.kind,
                used: i.entry.is_used(),
                status: i.status,
                original: i.entry.file.as_ref().map(|f| f.original.clone()),
                source: i.location.describe(package),
                packaged_as: i.packaged_as.clone(),
                files: i.files.len() as u64,
                bytes: i.files.iter().map(|&f| self.files[f].size).sum(),
            })
            .collect();
        let count = |s: AssetStatus| assets.iter().filter(|a| a.status == s).count();
        let stats = PackageStats {
            assets: assets.len(),
            packed: count(AssetStatus::Packed),
            missing: count(AssetStatus::Missing),
            excluded: count(AssetStatus::Excluded),
            files: self.files.len() as u64,
            asset_bytes: self.files.iter().map(|f| f.size).sum(),
            project_bytes,
            output_bytes: None,
        };
        PackageReport {
            kind,
            output: output.map(to_slash),
            assets,
            fonts: self
                .inventory
                .fonts
                .iter()
                .map(|f| FontReport {
                    name: f.name.clone(),
                    users: f.users.clone(),
                    embedded: false,
                })
                .collect(),
            external_projects: self.inventory.external_projects.clone(),
            warnings: self.warnings.clone(),
            stats,
        }
    }

    /// A copy of the project whose packed assets point into the package.
    fn packaged_project(
        &self,
        source: &PackSource<'_>,
        options: &PackOptions,
    ) -> Result<Project, PackageError> {
        let mut stage = copy_stage(source.stage)?;
        let relinks = self.relinks();
        if !relinks.is_empty() {
            let mut registry = CommandRegistry::new();
            registry.register(SetAssetFiles)?;
            let author = AuthorId::new("system:package").expect("valid author");
            registry.execute(
                &mut stage,
                &author,
                SetAssetFiles::ID,
                serde_json::json!({ "assets": relinks }),
            )?;
        }
        let mut project = Project::from_stage(stage);
        if let Some(m) = source.manifest {
            project.manifest = m.clone();
        }
        let note = PackageNote {
            tool: format!("bae-package {}", env!("CARGO_PKG_VERSION")),
            fonts: self
                .inventory
                .fonts
                .iter()
                .map(|f| f.name.clone())
                .collect(),
            trimmed: false,
            external_projects: self.inventory.external_projects.clone(),
            roots: options.roots.clone(),
        };
        project.manifest.extra.insert(
            MANIFEST_KEY.to_owned(),
            serde_json::to_value(note).expect("note serializes"),
        );
        Ok(project)
    }
}

/// Recorded in a packaged project's `project.json` under [`MANIFEST_KEY`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackageNote {
    /// What wrote the package.
    pub tool: String,
    /// Fonts the project uses (PostScript names); not embedded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<String>,
    /// Whether clips were trimmed.
    #[serde(default)]
    pub trimmed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_projects: Vec<String>,
    /// Packing scope (empty: whole project).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<PrimPath>,
}

/// Unique entry names inside the package.
#[derive(Debug, Default)]
struct Names {
    /// Lowercased taken names (files and sequence folders) → owner key.
    taken: HashMap<String, String>,
    /// Owner key → name, for files already placed.
    placed: HashMap<String, String>,
}

impl Names {
    /// `(name, fresh)` for one file; `fresh` is false when the same source was
    /// placed before (two assets sharing a file).
    fn place_file(&mut self, src: &FileSource) -> (String, bool) {
        let key = source_key(src);
        if let Some(name) = self.placed.get(&key) {
            return (name.clone(), false);
        }
        let file_name = sanitize(&src.file_name());
        let (stem, ext) = match file_name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
            _ => (file_name.clone(), String::new()),
        };
        let name = self.claim(&key, |n| {
            if n == 1 {
                format!("{ASSETS_DIR}{stem}{ext}")
            } else {
                format!("{ASSETS_DIR}{stem}_{n}{ext}")
            }
        });
        self.placed.insert(key, name.clone());
        (name, true)
    }

    /// Names for every frame of a sequence, in a folder of their own.
    fn place_sequence(&mut self, referenced: &str, frames: &[FileSource]) -> Vec<(String, bool)> {
        let key = format!("seq:{}", source_key(&frames[0]));
        if let Some(folder) = self.placed.get(&key).cloned() {
            return frames
                .iter()
                .map(|f| (format!("{folder}/{}", sanitize(&f.file_name())), false))
                .collect();
        }
        let base = sanitize(&sequence::folder_name(referenced));
        let folder = self.claim(&key, |n| {
            if n == 1 {
                format!("{ASSETS_DIR}{base}")
            } else {
                format!("{ASSETS_DIR}{base}_{n}")
            }
        });
        self.placed.insert(key, folder.clone());
        frames
            .iter()
            .map(|f| (format!("{folder}/{}", sanitize(&f.file_name())), true))
            .collect()
    }

    fn claim(&mut self, key: &str, candidate: impl Fn(u32) -> String) -> String {
        for n in 1.. {
            let name = candidate(n);
            let lower = name.to_lowercase();
            // A folder name must not clash with a file name or vice versa.
            let clash = self.taken.contains_key(&lower)
                || self.taken.keys().any(|t| {
                    t.starts_with(&format!("{lower}/")) || lower.starts_with(&format!("{t}/"))
                });
            if !clash {
                self.taken.insert(lower, key.to_owned());
                return name;
            }
        }
        unreachable!()
    }
}

fn source_key(src: &FileSource) -> String {
    match src {
        FileSource::Disk(p) => {
            let canonical = fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            format!("disk:{}", to_slash(&canonical))
        }
        FileSource::Packaged(e) => format!("pkg:{e}"),
    }
}

/// A file name that is valid on every platform and inside a zip.
fn sanitize(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    while s.ends_with(['.', ' ']) {
        s.pop();
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        s.insert(0, '_');
    }
    if s.is_empty() { "file".to_owned() } else { s }
}

/// An independent copy of a stage (same sublayers and history, no subscribers,
/// no undo stacks).
fn copy_stage(stage: &Stage) -> Result<Stage, PackageError> {
    let mut sublayers = Vec::new();
    for layer in stage.sublayers() {
        sublayers.push(Sublayer::from_snapshot(
            layer.id().clone(),
            &layer.export_snapshot()?,
        )?);
    }
    let mut copy = Stage::from_sublayers(sublayers)?;
    copy.set_edit_target(stage.edit_target())?;
    Ok(copy)
}

fn project_bytes(project: &Project) -> Result<u64, PackageError> {
    let mut total = serde_json::to_vec(&project.manifest)
        .map(|v| v.len() as u64)
        .unwrap_or(0);
    for layer in project.stage.sublayers() {
        total += layer.export_snapshot()?.len() as u64;
    }
    Ok(total)
}

/// What packing would do, without writing anything.
pub fn scan(source: &PackSource<'_>, options: &PackOptions) -> Result<PackageReport, PackageError> {
    let options = PackOptions {
        fail_on_missing: false,
        ..options.clone()
    };
    let plan = Plan::build(source, &options)?;
    let project = plan.packaged_project(source, &options)?;
    Ok(plan.report(ReportKind::Scan, None, project_bytes(&project)?))
}

/// Options for where the output goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Replace an existing `.baez` at the output path.
    pub overwrite: bool,
}

/// Write a self-contained `.baez`: the project and every (used) asset file.
pub fn pack(
    source: &PackSource<'_>,
    output: &Path,
    options: &PackOptions,
    write: WriteOptions,
) -> Result<PackageReport, PackageError> {
    let plan = Plan::build(source, options)?;
    if output.is_dir() {
        return Err(PackageError::OutputExists(output.to_owned()));
    }
    if output.exists() && !write.overwrite {
        return Err(PackageError::OutputExists(output.to_owned()));
    }
    guard_output(output, source.location, plan.disk_sources())?;
    let mut project = plan.packaged_project(source, options)?;
    let bytes = project_bytes(&project)?;

    // Embedded assets of a source `.baez` are copied out next to the output
    // first (the container writer reads files).
    let staging = StagingDir::new(output)?;
    let mut assets = Vec::with_capacity(plan.files.len());
    for f in &plan.files {
        let source = match &f.source {
            FileSource::Disk(p) => p.clone(),
            FileSource::Packaged(entry) => {
                let pkg = plan.locator.package().expect("packaged source");
                pkg.materialize(entry, staging.path()?)?
            }
        };
        assets.push(bae_model::BaezAsset {
            name: f.name.clone(),
            source,
        });
    }
    project.write_baez(output, &assets)?;
    drop(staging);

    let mut report = plan.report(ReportKind::Baez, Some(output), bytes);
    report.stats.output_bytes = fs::metadata(output).ok().map(|m| m.len());
    Ok(report)
}

/// Collect the project and its asset files into a `.bae` folder (AE *Collect
/// Files*). `out_dir` must not exist or be empty.
pub fn collect_files(
    source: &PackSource<'_>,
    out_dir: &Path,
    options: &PackOptions,
) -> Result<PackageReport, PackageError> {
    let plan = Plan::build(source, options)?;
    guard_output(out_dir, source.location, plan.disk_sources())?;
    let created = prepare_empty_dir(out_dir)?;
    let result = (|| {
        let mut project = plan.packaged_project(source, options)?;
        let bytes = project_bytes(&project)?;
        project.save_as(out_dir)?;
        for f in &plan.files {
            let dest = f.name.split('/').fold(out_dir.to_owned(), |p, s| p.join(s));
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(io_err(parent))?;
            }
            match &f.source {
                FileSource::Disk(src) => copy_preserving_mtime(src, &dest)?,
                FileSource::Packaged(entry) => plan
                    .locator
                    .package()
                    .expect("packaged source")
                    .extract_entry(entry, &dest)?,
            }
        }
        let mut report = plan.report(ReportKind::Folder, Some(out_dir), bytes);
        report.stats.output_bytes = Some(dir_size(out_dir));
        Ok(report)
    })();
    if result.is_err() && created {
        let _ = fs::remove_dir_all(out_dir);
    }
    result
}

/// Result of [`unpack`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UnpackReport {
    /// The `.bae` folder written.
    pub output: String,
    /// Files written.
    pub files: u64,
    pub bytes: u64,
    /// Assets of the unpacked project whose file cannot be found.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<PrimPath>,
}

/// Unpack a `.baez` into a `.bae` folder (`out_dir` must not exist or be empty)
/// and check that every asset resolves.
pub fn unpack(package: &Path, out_dir: &Path) -> Result<UnpackReport, PackageError> {
    if !package.is_file() || !is_baez(package) {
        return Err(bae_model::ProjectError::NotAProject(package.to_owned()).into());
    }
    let created = prepare_empty_dir(out_dir)?;
    let result = (|| {
        bae_model::extract_baez(package, out_dir)?;
        let (files, bytes) = count_files(out_dir);
        let opened = open_project(out_dir)?;
        Ok(UnpackReport {
            output: to_slash(out_dir),
            files,
            bytes,
            missing: opened.missing(),
        })
    })();
    if result.is_err() && created {
        let _ = fs::remove_dir_all(out_dir);
    }
    result
}

/// An open project with its asset locator — the way to open a `.baez` directly
/// (assets are read from inside it) or a `.bae` folder.
#[derive(Debug)]
pub struct OpenedProject {
    pub project: Project,
    pub assets: AssetLocator,
}

impl OpenedProject {
    /// Asset prims whose file cannot be found.
    pub fn missing(&self) -> Vec<PrimPath> {
        collect(&self.project.stage, &CollectOptions::default())
            .assets
            .into_iter()
            .filter(|a| {
                a.file
                    .as_ref()
                    .is_some_and(|f| self.assets.locate(f).is_missing())
            })
            .map(|a| a.path)
            .collect()
    }

    /// Where an asset prim's file is.
    pub fn locate(&self, asset: &PrimPath) -> AssetLocation {
        self.project
            .stage
            .prim(asset)
            .and_then(|p| AssetFile::from_prim(&p))
            .map_or(AssetLocation::Missing, |f| self.assets.locate(&f))
    }
}

/// Open a `.bae` folder or `.baez` file together with its asset locator.
pub fn open_project(path: impl AsRef<Path>) -> Result<OpenedProject, PackageError> {
    let project = Project::open_with(path, &OpenOptions::default())?;
    let assets = AssetLocator::for_project(&project)?;
    Ok(OpenedProject { project, assets })
}

/// Refuse outputs that are the project itself or one of the files being packed.
fn guard_output<'a>(
    output: &Path,
    location: Option<&Path>,
    sources: impl Iterator<Item = &'a Path>,
) -> Result<(), PackageError> {
    let out = normalize(output);
    let is_out = |p: &Path| normalize(p) == out;
    if let Some(loc) = location {
        let loc_n = normalize(loc);
        if loc_n == out || (loc.is_dir() && out.starts_with(&loc_n)) {
            return Err(PackageError::OutputIsSource(output.to_owned()));
        }
    }
    for s in sources {
        if is_out(s) {
            return Err(PackageError::OutputIsSource(output.to_owned()));
        }
    }
    Ok(())
}

/// Canonical path, also for paths that do not exist yet (canonical parent + name).
fn normalize(path: &Path) -> PathBuf {
    if let Ok(p) = fs::canonicalize(path) {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            normalize(parent).join(name)
        }
        _ => path.to_owned(),
    }
}

/// Make sure `dir` is an empty directory; `true` if it was created.
fn prepare_empty_dir(dir: &Path) -> Result<bool, PackageError> {
    if dir.exists() {
        let mut entries =
            fs::read_dir(dir).map_err(|_| PackageError::OutputExists(dir.to_owned()))?;
        if entries.next().is_some() {
            return Err(PackageError::OutputNotEmpty(dir.to_owned()));
        }
        return Ok(false);
    }
    fs::create_dir_all(dir).map_err(io_err(dir))?;
    Ok(true)
}

/// Copy a file and give the copy the source's modification time, so its
/// fingerprint matches the original.
fn copy_preserving_mtime(src: &Path, dest: &Path) -> Result<(), PackageError> {
    fs::copy(src, dest).map_err(io_err(src))?;
    if let Ok(mtime) = fs::metadata(src).and_then(|m| m.modified()) {
        let f = fs::OpenOptions::new()
            .write(true)
            .open(dest)
            .map_err(io_err(dest))?;
        f.set_modified(mtime).map_err(io_err(dest))?;
    }
    Ok(())
}

fn count_files(dir: &Path) -> (u64, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => {
                    files += 1;
                    bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
                _ => {}
            }
        }
    }
    (files, bytes)
}

fn dir_size(dir: &Path) -> u64 {
    count_files(dir).1
}

/// A temporary directory next to the output, removed on drop; created lazily.
struct StagingDir {
    path: PathBuf,
    created: std::cell::Cell<bool>,
}

impl StagingDir {
    fn new(output: &Path) -> Result<Self, PackageError> {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = output
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Ok(StagingDir {
            path: parent.join(format!(".{name}.staging-{}-{nanos:x}", std::process::id())),
            created: std::cell::Cell::new(false),
        })
    }

    fn path(&self) -> Result<&Path, PackageError> {
        if !self.created.get() {
            fs::create_dir_all(&self.path).map_err(io_err(&self.path))?;
            self.created.set(true);
        }
        Ok(&self.path)
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if self.created.get() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_shared_sources_reuse_them() {
        let mut n = Names::default();
        let a = FileSource::Disk(PathBuf::from("/nonexistent/x/a.mov"));
        let b = FileSource::Disk(PathBuf::from("/nonexistent/y/A.mov"));
        assert_eq!(n.place_file(&a), ("assets/a.mov".into(), true));
        assert_eq!(n.place_file(&b), ("assets/A_2.mov".into(), true));
        assert_eq!(n.place_file(&a), ("assets/a.mov".into(), false));
        let frames = [
            FileSource::Disk(PathBuf::from("/nonexistent/s/a_01.png")),
            FileSource::Disk(PathBuf::from("/nonexistent/s/a_02.png")),
        ];
        let placed = n.place_sequence("a_01.png", &frames);
        assert_eq!(placed[0], ("assets/a/a_01.png".into(), true));
        assert_eq!(placed[1], ("assets/a/a_02.png".into(), true));
        assert!(n.place_sequence("a_01.png", &frames).iter().all(|p| !p.1));
    }

    #[test]
    fn sanitizing() {
        assert_eq!(sanitize("a:b?.mov"), "a_b_.mov");
        assert_eq!(sanitize("con.png"), "_con.png");
        assert_eq!(sanitize("x. "), "x");
        assert_eq!(sanitize(""), "file");
    }
}
