//! Project files (ARCHITECTURE §4.6, §16): the `.bae` directory package and the
//! single-file `.baez` container.
//!
//! ```text
//! MyFilm.bae/
//!   project.json                  manifest: format, version, layer stack, path mappings
//!   sublayers/<sublayer-id>.loro  one Loro snapshot per sublayer ("anim/shot010" →
//!                                 sublayers/anim/shot010.loro)
//!   updates/<sublayer>/<client>.log   append-only update logs (bae-sync, reserved)
//!   assets/                       project-local assets (optional)
//!   cache/                        disposable
//! ```
//!
//! `.baez` is a zip of the same layout: project data deflated, files under `assets/`
//! stored uncompressed (zip64 for large media) so they can be read in place.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::asset::PathMapping;
use crate::error::ModelError;
use crate::ids::SublayerId;
use crate::path::PrimPath;
use crate::stage::Stage;
use crate::sublayer::Sublayer;

/// `format` value of `project.json`.
pub const PROJECT_FORMAT: &str = "bae.project";
/// Current `project.json` version. Bump it with a migration in [`MIGRATIONS`].
pub const PROJECT_VERSION: u32 = 1;
/// Extension of the single-file container.
pub const BAEZ_EXTENSION: &str = "baez";

const MANIFEST: &str = "project.json";
const SUBLAYERS: &str = "sublayers";
const SUBLAYER_EXT: &str = "loro";
/// Cross-project references nested deeper than this are not loaded.
const MAX_PROJECT_DEPTH: usize = 8;

/// Upgrades `project.json` from version `i + 1` to `i + 2`.
type Migration = fn(&mut Json) -> Result<(), String>;

/// Format migrations, oldest first. Empty while the format is at version 1.
const MIGRATIONS: &[Migration] = &[];

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{0} is not a BAE project (expected a .bae directory or a .baez file)")]
    NotAProject(PathBuf),
    #[error("invalid project.json: {0}")]
    Manifest(String),
    #[error(
        "project format version {found} is newer than this build supports ({supported}); update BAE"
    )]
    NewerFormat { found: u32, supported: u32 },
    #[error("migrating project.json from version {from}: {message}")]
    Migration { from: u32, message: String },
    #[error("sublayer {0} is listed in project.json but its file is missing")]
    MissingSublayer(SublayerId),
    #[error("zip: {0}")]
    Zip(String),
    #[error("the project has no location yet; use save_as")]
    NoLocation,
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl From<zip::result::ZipError> for ProjectError {
    fn from(e: zip::result::ZipError) -> Self {
        ProjectError::Zip(e.to_string())
    }
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> ProjectError + '_ {
    move |source| ProjectError::Io {
        path: path.to_owned(),
        source,
    }
}

/// `project.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub format: String,
    pub version: u32,
    /// Sublayer IDs, strongest first.
    pub layer_stack: Vec<SublayerId>,
    /// Team path mappings for assets (e.g. `Z:/` ↔ `/Volumes/NAS/`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_mappings: Vec<PathMapping>,
    /// Fields this build does not know; preserved on save.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Json>,
}

impl Default for ProjectManifest {
    fn default() -> Self {
        ProjectManifest {
            format: PROJECT_FORMAT.to_owned(),
            version: PROJECT_VERSION,
            layer_stack: vec![SublayerId::root()],
            path_mappings: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

/// Parse `project.json`, running migrations from older versions.
pub fn parse_manifest(bytes: &[u8]) -> Result<ProjectManifest, ProjectError> {
    let json: Json =
        serde_json::from_slice(bytes).map_err(|e| ProjectError::Manifest(e.to_string()))?;
    let json = migrate(json, MIGRATIONS)?;
    serde_json::from_value(json).map_err(|e| ProjectError::Manifest(e.to_string()))
}

fn migrate(mut json: Json, migrations: &[Migration]) -> Result<Json, ProjectError> {
    if json.get("format").and_then(Json::as_str) != Some(PROJECT_FORMAT) {
        return Err(ProjectError::Manifest(format!(
            "\"format\" must be \"{PROJECT_FORMAT}\""
        )));
    }
    let supported = migrations.len() as u32 + 1;
    let found =
        json.get("version")
            .and_then(Json::as_u64)
            .ok_or_else(|| ProjectError::Manifest("missing \"version\"".into()))? as u32;
    if found > supported {
        return Err(ProjectError::NewerFormat { found, supported });
    }
    for v in found.max(1)..supported {
        migrations[(v - 1) as usize](&mut json)
            .map_err(|message| ProjectError::Migration { from: v, message })?;
        json["version"] = Json::from(v + 1);
    }
    Ok(json)
}

/// Options for [`Project::open_with`].
#[derive(Clone, Debug)]
pub struct OpenOptions {
    /// Open the projects that cross-project references point to and attach them.
    pub load_references: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        OpenOptions {
            load_references: true,
        }
    }
}

/// An asset file to embed in a `.baez`.
#[derive(Clone, Debug)]
pub struct BaezAsset {
    /// Path inside the container, under `assets/` (e.g. `assets/footage/a.mov`).
    pub name: String,
    pub source: PathBuf,
}

/// A stage plus its manifest and location on disk.
#[derive(Debug)]
pub struct Project {
    pub stage: Stage,
    pub manifest: ProjectManifest,
    location: Option<PathBuf>,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

impl Project {
    /// A new, unsaved project with an empty `root` sublayer.
    pub fn new() -> Self {
        Project::from_stage(Stage::new())
    }

    pub fn from_stage(stage: Stage) -> Self {
        Project {
            stage,
            manifest: ProjectManifest::default(),
            location: None,
        }
    }

    /// The `.bae` directory or `.baez` file this project was opened from / saved to.
    pub fn location(&self) -> Option<&Path> {
        self.location.as_deref()
    }

    /// Directory that relative paths (assets, cross-project references) resolve
    /// against: the `.bae` directory itself, or the folder containing a `.baez`.
    pub fn base_dir(&self) -> Option<&Path> {
        let loc = self.location.as_deref()?;
        if is_baez(loc) {
            loc.parent()
        } else {
            Some(loc)
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        Self::open_with(path, &OpenOptions::default())
    }

    pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> Result<Self, ProjectError> {
        let mut chain = Vec::new();
        open_inner(path.as_ref(), options, &mut chain)
    }

    /// Save to the current location.
    pub fn save(&mut self) -> Result<(), ProjectError> {
        let loc = self.location.clone().ok_or(ProjectError::NoLocation)?;
        self.save_to(&loc, &[])
    }

    /// Save to `path` (a `.baez` file if it has that extension, otherwise a `.bae`
    /// directory) and make it the project's location.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), ProjectError> {
        let path = path.as_ref().to_owned();
        self.save_to(&path, &[])?;
        self.location = Some(path);
        Ok(())
    }

    /// Write a `.baez` with embedded assets, without changing the location.
    pub fn write_baez(
        &mut self,
        path: impl AsRef<Path>,
        assets: &[BaezAsset],
    ) -> Result<(), ProjectError> {
        self.sync_manifest();
        write_baez(path.as_ref(), &self.manifest, &self.stage, assets)
    }

    fn save_to(&mut self, path: &Path, assets: &[BaezAsset]) -> Result<(), ProjectError> {
        self.sync_manifest();
        if is_baez(path) {
            write_baez(path, &self.manifest, &self.stage, assets)
        } else {
            write_dir(path, &self.manifest, &self.stage)
        }
    }

    fn sync_manifest(&mut self) {
        self.manifest.format = PROJECT_FORMAT.to_owned();
        self.manifest.version = PROJECT_VERSION;
        self.manifest.layer_stack = self.stage.layer_stack().to_vec();
    }
}

fn is_baez(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(BAEZ_EXTENSION)
}

/// File name of a sublayer inside the package (`/` in IDs become directories).
pub fn sublayer_file(id: &SublayerId) -> String {
    format!("{SUBLAYERS}/{}.{SUBLAYER_EXT}", id.as_str())
}

fn open_inner(
    path: &Path,
    options: &OpenOptions,
    chain: &mut Vec<PathBuf>,
) -> Result<Project, ProjectError> {
    let (manifest, blobs) = if path.is_dir() {
        read_dir(path)?
    } else if is_baez(path) && path.is_file() {
        read_baez(path)?
    } else {
        return Err(ProjectError::NotAProject(path.to_owned()));
    };
    let mut sublayers = Vec::with_capacity(manifest.layer_stack.len());
    for id in &manifest.layer_stack {
        let bytes = blobs
            .get(id)
            .ok_or_else(|| ProjectError::MissingSublayer(id.clone()))?;
        sublayers.push(Sublayer::from_snapshot(id.clone(), bytes)?);
    }
    let mut project = Project {
        stage: Stage::from_sublayers(sublayers)?,
        manifest,
        location: Some(path.to_owned()),
    };
    if options.load_references && chain.len() < MAX_PROJECT_DEPTH {
        chain.push(fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()));
        let base = project.base_dir().map(Path::to_owned).unwrap_or_default();
        for key in referenced_projects(&project.stage) {
            let target = base.join(&key);
            let canonical = fs::canonicalize(&target).unwrap_or_else(|_| target.clone());
            if chain.contains(&canonical) {
                continue;
            }
            // Missing or broken referenced projects stay unresolved instead of
            // failing the open; `Stage::unresolved_references` reports them.
            if let Ok(other) = open_inner(&target, options, chain) {
                project.stage.attach_project(key, other.stage);
            }
        }
        chain.pop();
    }
    Ok(project)
}

/// Distinct `Reference::project` keys used in the local layer stack.
fn referenced_projects(stage: &Stage) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for layer in stage.sublayers() {
        for path in layer.paths() {
            for r in &layer.spec(path).expect("listed").references {
                if let Some(k) = &r.project
                    && !keys.contains(k)
                {
                    keys.push(k.clone());
                }
            }
        }
    }
    keys.sort();
    keys
}

type Blobs = BTreeMap<SublayerId, Vec<u8>>;

fn read_dir(dir: &Path) -> Result<(ProjectManifest, Blobs), ProjectError> {
    let manifest_path = dir.join(MANIFEST);
    if !manifest_path.is_file() {
        return Err(ProjectError::NotAProject(dir.to_owned()));
    }
    let manifest = parse_manifest(&fs::read(&manifest_path).map_err(io_err(&manifest_path))?)?;
    let mut blobs = Blobs::new();
    for id in &manifest.layer_stack {
        let file = dir.join(sublayer_file(id));
        if file.is_file() {
            blobs.insert(id.clone(), fs::read(&file).map_err(io_err(&file))?);
        }
    }
    Ok((manifest, blobs))
}

fn write_dir(dir: &Path, manifest: &ProjectManifest, stage: &Stage) -> Result<(), ProjectError> {
    fs::create_dir_all(dir.join(SUBLAYERS)).map_err(io_err(dir))?;
    let mut keep: Vec<PathBuf> = Vec::new();
    for layer in stage.sublayers() {
        let file = dir.join(sublayer_file(layer.id()));
        write_atomic(&file, &layer.export_snapshot()?)?;
        keep.push(file);
    }
    // Sublayers removed from the stack: drop their stale snapshots.
    remove_stale(&dir.join(SUBLAYERS), &keep)?;
    let json = serde_json::to_vec_pretty(manifest).expect("manifest serializes");
    write_atomic(&dir.join(MANIFEST), &json)
}

fn remove_stale(dir: &Path, keep: &[PathBuf]) -> Result<(), ProjectError> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_stale(&path, keep)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(SUBLAYER_EXT)
            && !keep.contains(&path)
        {
            fs::remove_file(&path).map_err(io_err(&path))?;
        }
    }
    Ok(())
}

/// Write via a temporary file and rename, so a crash never leaves a half-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut f = fs::File::create(&tmp).map_err(io_err(&tmp))?;
    f.write_all(bytes).map_err(io_err(&tmp))?;
    f.sync_all().map_err(io_err(&tmp))?;
    drop(f);
    fs::rename(&tmp, path).map_err(io_err(path))
}

fn read_baez(path: &Path) -> Result<(ProjectManifest, Blobs), ProjectError> {
    let file = fs::File::open(path).map_err(io_err(path))?;
    let mut zip = ZipArchive::new(file)?;
    let manifest = parse_manifest(
        &read_entry(&mut zip, MANIFEST)?
            .ok_or_else(|| ProjectError::NotAProject(path.to_owned()))?,
    )?;
    let mut blobs = Blobs::new();
    for id in &manifest.layer_stack {
        if let Some(bytes) = read_entry(&mut zip, &sublayer_file(id))? {
            blobs.insert(id.clone(), bytes);
        }
    }
    Ok((manifest, blobs))
}

fn read_entry<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>, ProjectError> {
    match zip.by_name(name) {
        Ok(mut f) => {
            let mut buf = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut buf)
                .map_err(|e| ProjectError::Zip(e.to_string()))?;
            Ok(Some(buf))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_baez(
    path: &Path,
    manifest: &ProjectManifest,
    stage: &Stage,
    assets: &[BaezAsset],
) -> Result<(), ProjectError> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let file = fs::File::create(&tmp).map_err(io_err(&tmp))?;
    let mut zip = ZipWriter::new(io::BufWriter::new(file));
    let data = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let media = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true);

    zip.start_file(MANIFEST, data)?;
    zip.write_all(&serde_json::to_vec_pretty(manifest).expect("manifest serializes"))
        .map_err(io_err(&tmp))?;
    for layer in stage.sublayers() {
        zip.start_file(sublayer_file(layer.id()), data)?;
        zip.write_all(&layer.export_snapshot()?)
            .map_err(io_err(&tmp))?;
    }
    for asset in assets {
        if !asset.name.starts_with("assets/")
            || asset.name.split('/').any(|s| s == ".." || s.is_empty())
        {
            return Err(ProjectError::Zip(format!(
                "invalid asset name {:?}",
                asset.name
            )));
        }
        zip.start_file(asset.name.as_str(), media)?;
        let mut src = fs::File::open(&asset.source).map_err(io_err(&asset.source))?;
        io::copy(&mut src, &mut zip).map_err(io_err(&asset.source))?;
    }
    let mut out = zip.finish()?;
    out.flush().map_err(io_err(&tmp))?;
    drop(out);
    fs::rename(&tmp, path).map_err(io_err(path))
}

/// Unpack a `.baez` into a `.bae` directory (project data and embedded assets).
pub fn extract_baez(path: impl AsRef<Path>, dir: impl AsRef<Path>) -> Result<(), ProjectError> {
    let (path, dir) = (path.as_ref(), dir.as_ref());
    let file = fs::File::open(path).map_err(io_err(path))?;
    let mut zip = ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dir.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(io_err(&out))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        let mut f = fs::File::create(&out).map_err(io_err(&out))?;
        io::copy(&mut entry, &mut f).map_err(io_err(&out))?;
    }
    Ok(())
}

/// Readable JSON of one sublayer (`bae export-json`): its prim specs as trees.
pub fn export_sublayer_json(stage: &Stage, id: &SublayerId) -> Result<Json, ModelError> {
    let layer = stage
        .sublayer(id)
        .ok_or_else(|| ModelError::UnknownSublayer(id.clone()))?;
    Ok(serde_json::json!({
        "format": "bae.export",
        "version": 1,
        "sublayer": id,
        "prims": layer.to_trees(),
    }))
}

/// Readable JSON of the composed stage (all sublayers flattened, references resolved).
pub fn export_flattened_json(stage: &Stage) -> Json {
    serde_json::json!({
        "format": "bae.export",
        "version": 1,
        "flattened": true,
        "layer_stack": stage.layer_stack(),
        "prims": stage.flatten(),
    })
}

/// Readable JSON of a subtree of the composed stage.
pub fn export_prim_json(stage: &Stage, path: &PrimPath) -> Result<Json, ModelError> {
    stage.require_prim(path)?;
    let tree = stage
        .flatten()
        .into_iter()
        .find_map(|t| find_tree(t, &PrimPath::root(), path));
    Ok(serde_json::json!({
        "format": "bae.export",
        "version": 1,
        "flattened": true,
        "path": path,
        "prims": tree.into_iter().collect::<Vec<_>>(),
    }))
}

fn find_tree(
    tree: crate::prim::PrimTree,
    parent: &PrimPath,
    want: &PrimPath,
) -> Option<crate::prim::PrimTree> {
    let here = parent.child_unchecked(&tree.name);
    if &here == want {
        return Some(tree);
    }
    if !here.is_ancestor_of(want) {
        return None;
    }
    tree.children
        .into_iter()
        .find_map(|c| find_tree(c, &here, want))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bump(json: &mut Json) -> Result<(), String> {
        let stack = json["layers"].take();
        json["layer_stack"] = stack;
        Ok(())
    }

    #[test]
    fn migrations_run_in_order_and_newer_versions_are_refused() {
        let old = serde_json::json!({"format": PROJECT_FORMAT, "version": 1, "layers": ["root"]});
        let migrated = migrate(old, &[bump as Migration]).unwrap();
        assert_eq!(migrated["version"], 2);
        assert_eq!(migrated["layer_stack"], serde_json::json!(["root"]));

        let newer = serde_json::json!({"format": PROJECT_FORMAT, "version": 7, "layer_stack": []});
        assert!(matches!(
            parse_manifest(newer.to_string().as_bytes()),
            Err(ProjectError::NewerFormat {
                found: 7,
                supported: 1
            })
        ));
        let wrong = serde_json::json!({"format": "other", "version": 1});
        assert!(matches!(
            parse_manifest(wrong.to_string().as_bytes()),
            Err(ProjectError::Manifest(_))
        ));
    }

    #[test]
    fn unknown_manifest_fields_survive() {
        let json = serde_json::json!({
            "format": PROJECT_FORMAT, "version": 1, "layer_stack": ["root"],
            "future_setting": {"x": 1}
        });
        let m = parse_manifest(json.to_string().as_bytes()).unwrap();
        assert_eq!(m.extra["future_setting"]["x"], 1);
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["future_setting"]["x"], 1);
    }

    #[test]
    fn sublayer_files_nest_by_id() {
        assert_eq!(
            sublayer_file(&SublayerId::new("anim/shot010").unwrap()),
            "sublayers/anim/shot010.loro"
        );
    }
}
