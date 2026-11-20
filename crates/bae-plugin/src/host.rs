//! The plugin host (spec §6, §7): discovery, version selection, registration
//! into the effect registries and runtime unloading.
//!
//! Plugin effects are registered into the same registries as built-in effects:
//! definitions into `bae_model::EffectRegistry` (UI, agents, AE import,
//! placeholders) and implementations into `bae_render::EffectLibrary` (render
//! graph). Unloading removes exactly the plugin's entries; render graphs that
//! still hold an `Arc` of an effect keep it alive until they are rebuilt, and
//! its GPU resources are freed with the last `Arc` (spec §7.3, §7.4). Effects
//! of an unloaded plugin in open projects become placeholders
//! (`EffectStatus::MissingPlugin`) without any document change (spec §20).
//!
//! Plugin API 0.1 loads declarative WGSL effects. Native and script
//! contributions are validated and listed but not executed yet; they are
//! reported as warnings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_app::{Owner, Registry};
use bae_model::{EffectDef, EffectRegistry};
use bae_render::{Effect, EffectLibrary};

use crate::declarative::DeclarativeEffect;
use crate::diag::Diagnostic;
use crate::manifest::{EffectImplementation, MANIFEST_FILE, PluginManifest};
use crate::package::{DirSource, Package, PackageSource};

/// Where a plugin was found. Ties between equal versions go to the more
/// specific scope (spec §6.4): project > user > system > bundled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Shipped with the application.
    Bundled,
    /// Installed for all users.
    System,
    /// Installed for the current user.
    User,
    /// Inside the open project (`<Project>.bae/plugins/`) or a development
    /// directory (`--plugin-dir`, `BAE_PLUGIN_PATH`).
    Project,
}

/// A directory to search for plugin packages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDir {
    /// The directory.
    pub path: PathBuf,
    /// Its scope.
    pub scope: Scope,
}

impl PluginDir {
    /// A search directory.
    pub fn new(path: impl Into<PathBuf>, scope: Scope) -> Self {
        Self {
            path: path.into(),
            scope,
        }
    }
}

/// A package found by [`PluginHost::scan`] (manifest only, no shader compiled).
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Package root.
    pub path: PathBuf,
    /// Scope of the directory it was found in.
    pub scope: Scope,
    /// The parsed, validated manifest.
    pub manifest: PluginManifest,
}

/// Result of a scan: candidates and problems by package path.
#[derive(Clone, Debug, Default)]
pub struct ScanReport {
    /// Valid packages.
    pub candidates: Vec<Candidate>,
    /// Packages that failed, with their diagnostics.
    pub failed: Vec<(PathBuf, Vec<Diagnostic>)>,
}

/// A loaded plugin.
#[derive(Debug)]
pub struct LoadedPlugin {
    /// The package (manifest, effect definitions, shader sources).
    pub package: Package,
    /// Scope it was loaded from.
    pub scope: Scope,
    /// Declarative effects registered in the effect library.
    pub effects: Vec<Arc<DeclarativeEffect>>,
    /// Warnings at load (e.g. contributions this host does not run yet).
    pub warnings: Vec<Diagnostic>,
}

impl LoadedPlugin {
    /// The plugin id.
    pub fn id(&self) -> &str {
        self.package.id()
    }
}

/// What [`PluginHost::unload`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnloadReport {
    /// Effect definitions removed from the effect registry.
    pub definitions_removed: usize,
    /// Implementations removed from the effect library.
    pub effects_removed: usize,
    /// Effects still referenced by someone else (render graphs being built or
    /// executed); they are freed when those references drop.
    pub effects_in_use: usize,
}

/// Errors of [`PluginHost::load`].
#[derive(Clone, Debug, thiserror::Error)]
pub enum LoadError {
    /// The package is invalid.
    #[error("{} problem(s) in the package: {}", .0.len(), .0.first().map(ToString::to_string).unwrap_or_default())]
    Invalid(Vec<Diagnostic>),
    /// An effect id is already registered by someone else (a built-in or another plugin).
    #[error("effect `{match_name}` is already registered by {owner}")]
    Conflict {
        /// The effect.
        match_name: String,
        /// Who registered it.
        owner: String,
    },
}

/// Where the host registers plugin effects: the same registries as built-in
/// effects (spec §19). Implemented for plain registries ([`EffectRegistries`])
/// and for the application's runtime registries ([`AppEffectRegistries`]).
pub trait EffectRegistrar {
    /// Who registered `match_name`, when that is anyone but plugin `plugin_id`.
    fn foreign_owner(&self, match_name: &str, plugin_id: &str) -> Option<String>;

    /// Registers one effect of plugin `plugin_id` (replacing its own earlier entry).
    fn register(&mut self, plugin_id: &str, def: EffectDef, effect: Arc<dyn Effect>);

    /// Removes every effect of plugin `plugin_id` (`match_names` are the ones it
    /// registered). Returns (definitions, implementations) removed.
    fn unregister(&mut self, plugin_id: &str, match_names: &[String]) -> (usize, usize);
}

/// Plain effect registries: definitions for the document model and
/// implementations for a renderer (tests, tools, a renderer outside the app).
#[derive(Debug, Default)]
pub struct EffectRegistries {
    /// Effect definitions.
    pub definitions: EffectRegistry,
    /// Effect implementations.
    pub library: EffectLibrary,
}

impl EffectRegistrar for EffectRegistries {
    fn foreign_owner(&self, match_name: &str, plugin_id: &str) -> Option<String> {
        let existing = self.definitions.get(match_name)?;
        match existing.plugin.as_ref().map(|p| p.id.as_str()) {
            Some(owner) if owner == plugin_id => None,
            Some(owner) => Some(format!("plugin {owner}")),
            None => Some("a built-in effect".to_owned()),
        }
    }

    fn register(&mut self, _plugin_id: &str, def: EffectDef, effect: Arc<dyn Effect>) {
        self.definitions.register(def);
        self.library.register(effect);
    }

    fn unregister(&mut self, plugin_id: &str, match_names: &[String]) -> (usize, usize) {
        let defs = self.definitions.unregister_plugin(plugin_id);
        let effects = match_names
            .iter()
            .filter(|m| self.library.unregister(m).is_some())
            .count();
        (defs, effects)
    }
}

/// The application's runtime registries (`bae_app::Registry`), where built-in
/// modules register their effects too. Plugin entries are owned by
/// `Owner::Plugin(<plugin-id>)`, so `App::unregister_owner` also removes them.
#[derive(Clone, Debug)]
pub struct AppEffectRegistries {
    /// Effect definitions by match name (UI, agents, AE import, placeholders).
    pub definitions: Arc<Registry<EffectDef>>,
    /// Effect implementations by match name (render graph).
    pub effects: Arc<Registry<dyn Effect>>,
}

impl EffectRegistrar for AppEffectRegistries {
    fn foreign_owner(&self, match_name: &str, plugin_id: &str) -> Option<String> {
        let mine = Owner::plugin(plugin_id);
        let defs = self.definitions.snapshot();
        let effects = self.effects.snapshot();
        let owner = defs
            .entry(match_name)
            .map(|e| e.owner().clone())
            .or_else(|| effects.entry(match_name).map(|e| e.owner().clone()))?;
        (owner != mine).then(|| owner.to_string())
    }

    fn register(&mut self, plugin_id: &str, def: EffectDef, effect: Arc<dyn Effect>) {
        let owner = Owner::plugin(plugin_id);
        let id = def.match_name.clone();
        // Conflicts were checked by the host; replace only our own entries.
        if let Err(error) = self
            .definitions
            .replace(owner.clone(), id.as_str(), Arc::new(def))
        {
            tracing::warn!("plugin {plugin_id}: effect definition `{id}` not registered: {error}");
        }
        if let Err(error) = self.effects.replace(owner, id.as_str(), effect) {
            tracing::warn!("plugin {plugin_id}: effect `{id}` not registered: {error}");
        }
    }

    fn unregister(&mut self, plugin_id: &str, _match_names: &[String]) -> (usize, usize) {
        let owner = Owner::plugin(plugin_id);
        // The returned entries are dropped here so that only render graphs
        // still hold the effects afterwards.
        let defs = self.definitions.unregister_owner(&owner).len();
        let effects = self.effects.unregister_owner(&owner).len();
        (defs, effects)
    }
}

/// The plugin host: owns loaded plugins and keeps the effect registries in sync.
#[derive(Debug, Default)]
pub struct PluginHost {
    plugins: BTreeMap<String, LoadedPlugin>,
}

impl PluginHost {
    /// A host with no plugins.
    pub fn new() -> Self {
        Self::default()
    }

    /// Package roots under `dir`: `dir` itself if it holds a manifest, its
    /// subdirectories with a manifest, and `<id>/<version>/` installs.
    pub fn discover(dir: &Path) -> Vec<PathBuf> {
        let has_manifest = |p: &Path| p.join(MANIFEST_FILE).is_file();
        if has_manifest(dir) {
            return vec![dir.to_owned()];
        }
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        let mut subdirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !is_hidden(p))
            .collect();
        subdirs.sort();
        for sub in subdirs {
            if has_manifest(&sub) {
                out.push(sub);
            } else if let Ok(versions) = std::fs::read_dir(&sub) {
                let mut v: Vec<PathBuf> = versions
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && !is_hidden(p) && has_manifest(p))
                    .collect();
                v.sort();
                out.extend(v);
            }
        }
        out
    }

    /// Reads and validates the manifests of every package in `dirs` (no shader
    /// is compiled, no code loaded: spec §6.2).
    pub fn scan(dirs: &[PluginDir]) -> ScanReport {
        let mut report = ScanReport::default();
        for dir in dirs {
            for path in Self::discover(&dir.path) {
                let parsed = std::fs::read_to_string(path.join(MANIFEST_FILE))
                    .map_err(|e| {
                        vec![Diagnostic::error(
                            MANIFEST_FILE,
                            format!("cannot read: {e}"),
                        )]
                    })
                    .and_then(|text| PluginManifest::parse(&text).map_err(|e| vec![e.0]));
                match parsed {
                    Ok(manifest) => {
                        let diags = manifest.validate();
                        if crate::diag::has_errors(&diags) {
                            report.failed.push((path, diags));
                        } else {
                            report.candidates.push(Candidate {
                                path,
                                scope: dir.scope,
                                manifest,
                            });
                        }
                    }
                    Err(diags) => report.failed.push((path, diags)),
                }
            }
        }
        report
    }

    /// Picks the active candidate per plugin id: the highest version, ties
    /// to the more specific scope (spec §6.4 rules 2; the project lock and
    /// dependency backtracking come later).
    pub fn select(candidates: &[Candidate]) -> Vec<&Candidate> {
        let mut best: BTreeMap<&str, &Candidate> = BTreeMap::new();
        for c in candidates {
            let id = c.manifest.plugin.id.as_str();
            let better = match best.get(id) {
                None => true,
                Some(b) => {
                    (&c.manifest.plugin.version, c.scope) > (&b.manifest.plugin.version, b.scope)
                }
            };
            if better {
                best.insert(id, c);
            }
        }
        best.into_values().collect()
    }

    /// Scans `dirs`, selects the active versions and loads them. Returns the
    /// ids loaded and every problem found (by package path).
    pub fn load_dirs(
        &mut self,
        dirs: &[PluginDir],
        target: &mut dyn EffectRegistrar,
    ) -> (Vec<String>, Vec<(PathBuf, Vec<Diagnostic>)>) {
        let report = Self::scan(dirs);
        let mut problems = report.failed;
        let mut loaded = Vec::new();
        for c in Self::select(&report.candidates) {
            match self.load_from(&DirSource::new(&c.path), c.scope, target) {
                Ok(p) => loaded.push(p.id().to_owned()),
                Err(LoadError::Invalid(d)) => problems.push((c.path.clone(), d)),
                Err(e) => problems.push((
                    c.path.clone(),
                    vec![Diagnostic::error(MANIFEST_FILE, e.to_string())],
                )),
            }
        }
        (loaded, problems)
    }

    /// Loads one package and registers its effects. A plugin with the same id
    /// that is already loaded is unloaded first (update without restart).
    pub fn load_from(
        &mut self,
        source: &dyn PackageSource,
        scope: Scope,
        target: &mut dyn EffectRegistrar,
    ) -> Result<&LoadedPlugin, LoadError> {
        let package = Package::load(source).map_err(LoadError::Invalid)?;
        self.load_package(package, scope, target)
    }

    /// Registers an already loaded package.
    pub fn load_package(
        &mut self,
        package: Package,
        scope: Scope,
        target: &mut dyn EffectRegistrar,
    ) -> Result<&LoadedPlugin, LoadError> {
        let id = package.id().to_owned();
        // Conflicts with entries owned by someone else.
        for e in &package.effects {
            if let Some(owner) = target.foreign_owner(&e.def.match_name, &id) {
                return Err(LoadError::Conflict {
                    match_name: e.def.match_name.clone(),
                    owner,
                });
            }
        }
        let effects = DeclarativeEffect::from_package(&package).map_err(LoadError::Invalid)?;
        let mut warnings = package.warnings.clone();
        for e in &package.effects {
            if e.decl.implementation == EffectImplementation::Native {
                warnings.push(
                    Diagnostic::warning(
                        MANIFEST_FILE,
                        "native effects are listed but not executed by plugin API 0.1 hosts yet; instances show as placeholders",
                    )
                    .in_context(format!("effect \"{}\"", e.decl.id)),
                );
            }
        }
        if !package.manifest.commands.is_empty() || !package.manifest.automations.is_empty() {
            warnings.push(Diagnostic::warning(
                MANIFEST_FILE,
                "script commands and automation scripts are validated but not run by plugin API 0.1 hosts yet",
            ));
        }

        self.unload(&id, target);
        for effect in &effects {
            target.register(&id, effect.def().clone(), effect.clone() as Arc<dyn Effect>);
        }
        let plugin = LoadedPlugin {
            package,
            scope,
            effects,
            warnings,
        };
        Ok(self.plugins.entry(id).insert_entry(plugin).into_mut())
    }

    /// Unloads a plugin: removes its effect definitions and implementations.
    /// `None` if it was not loaded.
    pub fn unload(&mut self, id: &str, target: &mut dyn EffectRegistrar) -> Option<UnloadReport> {
        let plugin = self.plugins.remove(id)?;
        let names: Vec<String> = plugin
            .effects
            .iter()
            .map(|e| e.def().match_name.clone())
            .collect();
        let (definitions_removed, effects_removed) = target.unregister(id, &names);
        let effects_in_use = plugin
            .effects
            .iter()
            // Our own handle is the only one left unless a graph still holds it.
            .filter(|e| Arc::strong_count(e) > 1)
            .count();
        Some(UnloadReport {
            definitions_removed,
            effects_removed,
            effects_in_use,
        })
    }

    /// Unloads every plugin.
    pub fn unload_all(&mut self, target: &mut dyn EffectRegistrar) {
        let ids: Vec<String> = self.plugins.keys().cloned().collect();
        for id in ids {
            self.unload(&id, target);
        }
    }

    /// A loaded plugin.
    pub fn get(&self, id: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(id)
    }

    /// Loaded plugins, by id.
    pub fn plugins(&self) -> impl ExactSizeIterator<Item = &LoadedPlugin> + '_ {
        self.plugins.values()
    }
}

fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}
