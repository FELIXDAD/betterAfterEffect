//! Finding footage files for evaluated layers: on disk (relative path, original
//! path, path mappings, search) or inside the `.baez` the project was opened
//! from, through 21's `bae_package::AssetLocator`. Packaged entries are
//! extracted once into a cache directory (decoders open files by path); an
//! image sequence's whole folder is extracted with its first frame.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use bae_model::{AssetFile, PathMapping, PrimView, Project};
use bae_package::{AssetLocation, AssetLocator};
use rustc_hash::FxHasher;

use crate::error::RenderError;

/// Resolves `Asset` prims to files the decoders can open.
#[derive(Clone, Debug)]
pub struct AssetFiles {
    locator: AssetLocator,
    cache_dir: PathBuf,
}

impl AssetFiles {
    /// Files relative to `base_dir` (no package).
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self::from_locator(AssetLocator::new(base_dir, Vec::new()), None)
    }

    /// The files of an open project: its base directory, path mappings and, when
    /// it was opened from a `.baez`, the embedded assets.
    pub fn for_project(project: &Project) -> Result<Self, RenderError> {
        let locator =
            AssetLocator::for_project(project).map_err(|e| RenderError::Other(e.to_string()))?;
        Ok(Self::from_locator(locator, project.location()))
    }

    /// The files of a project at `location` (`.bae` or `.baez`).
    pub fn for_location(location: &Path, mappings: Vec<PathMapping>) -> Result<Self, RenderError> {
        let locator = AssetLocator::for_location(Some(location), mappings)
            .map_err(|e| RenderError::Other(e.to_string()))?;
        Ok(Self::from_locator(locator, Some(location)))
    }

    fn from_locator(locator: AssetLocator, location: Option<&Path>) -> Self {
        let mut h = FxHasher::default();
        location.hash(&mut h);
        let cache_dir = std::env::temp_dir()
            .join("bae-render-assets")
            .join(format!("{:016x}", h.finish()));
        Self { locator, cache_dir }
    }

    /// Extract packaged assets under `dir` instead of the temporary directory.
    #[must_use]
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = dir;
        self
    }

    /// The locator (for reports and relinking).
    pub fn locator(&self) -> &AssetLocator {
        &self.locator
    }

    /// The file for an `Asset` prim: `Ok(None)` when it is missing, `Err` when a
    /// packaged entry could not be extracted.
    pub fn file(&self, asset: &PrimView<'_>) -> Result<Option<PathBuf>, String> {
        let Some(reference) = AssetFile::from_prim(asset) else {
            return Ok(None);
        };
        self.resolve_file(&reference)
    }

    pub(crate) fn resolve_file(&self, reference: &AssetFile) -> Result<Option<PathBuf>, String> {
        match self.locator.locate(reference) {
            AssetLocation::File { path, .. } => Ok(Some(path)),
            AssetLocation::Missing => Ok(None),
            AssetLocation::Packaged { entry } => self.materialize(&entry).map(Some),
        }
    }

    fn materialize(&self, entry: &str) -> Result<PathBuf, String> {
        let package = self
            .locator
            .package()
            .ok_or_else(|| format!("{entry}: no package is open"))?;
        let err = |e: bae_package::PackageError| format!("{entry}: {e}");
        // Sequences are packed in a folder of their own (`assets/<name>/frame`):
        // extract every frame next to the first one.
        if let Some((folder, _)) = entry.rsplit_once('/')
            && folder != bae_package::container::ASSETS_DIR.trim_end_matches('/')
        {
            let prefix = format!("{folder}/");
            let siblings: Vec<String> = package
                .entries()
                .filter(|e| e.name.starts_with(&prefix) && e.name != entry)
                .map(|e| e.name.clone())
                .collect();
            for name in siblings {
                package.materialize(&name, &self.cache_dir).map_err(err)?;
            }
        }
        package.materialize(entry, &self.cache_dir).map_err(err)
    }
}
