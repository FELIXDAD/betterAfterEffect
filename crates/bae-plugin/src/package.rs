//! Loading and checking a plugin package (spec §5.1, §5.4).
//!
//! [`Package::load`] reads the manifest, validates it, checks that referenced
//! files exist and compiles every declarative effect against its generated
//! prelude. It is what `bae plugin check` runs and what the host runs before
//! registering anything. Packages come from a directory ([`DirSource`]) or from
//! memory ([`MemorySource`]: built-in effects embedded with `include_str!`,
//! spec §19).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use bae_model::EffectDef;

use crate::diag::{Diagnostic, has_errors};
use crate::effect_def::effect_def;
use crate::manifest::{EffectDecl, EffectImplementation, MANIFEST_FILE, PluginManifest};
use crate::wgsl::{self, SourceFile, WorkingFormat};

/// Where package files come from.
pub trait PackageSource {
    /// Reads a package-relative UTF-8 file.
    fn read_text(&self, path: &str) -> io::Result<String>;

    /// `true` if the package-relative file exists.
    fn exists(&self, path: &str) -> bool;

    /// Human-readable location for logs (`/path/to/pkg`, `builtin:bae.effects`).
    fn describe(&self) -> String;

    /// The package directory on disk (native libraries load from it), if any.
    fn root(&self) -> Option<&Path> {
        None
    }
}

/// A package directory on disk.
#[derive(Clone, Debug)]
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    /// A package rooted at `root` (the directory containing `bae-plugin.toml`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The package root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &str) -> io::Result<PathBuf> {
        crate::id::check_package_path(path)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        Ok(path.split('/').fold(self.root.clone(), |p, s| p.join(s)))
    }
}

impl PackageSource for DirSource {
    fn read_text(&self, path: &str) -> io::Result<String> {
        std::fs::read_to_string(self.resolve(path)?)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_ok_and(|p| p.is_file())
    }

    fn describe(&self) -> String {
        self.root.display().to_string()
    }

    fn root(&self) -> Option<&Path> {
        Some(&self.root)
    }
}

/// A package held in memory (path → contents).
#[derive(Clone, Debug, Default)]
pub struct MemorySource {
    name: String,
    files: BTreeMap<String, String>,
}

impl MemorySource {
    /// An empty package named `name` (for logs, e.g. `builtin:bae.effects`).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            files: BTreeMap::new(),
        }
    }

    /// Adds a file.
    pub fn with_file(mut self, path: impl Into<String>, text: impl Into<String>) -> Self {
        self.files.insert(path.into(), text.into());
        self
    }
}

impl PackageSource for MemorySource {
    fn read_text(&self, path: &str) -> io::Result<String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn describe(&self) -> String {
        self.name.clone()
    }
}

/// One effect of a loaded package.
#[derive(Clone, Debug)]
pub struct EffectPackage {
    /// The manifest declaration.
    pub decl: EffectDecl,
    /// The definition registered in `bae_model::EffectRegistry`
    /// (`match_name` = `<plugin-id>/<effect-id>`).
    pub def: EffectDef,
    /// Declarative effects: includes then shader, as compiled.
    pub shader_files: Vec<SourceFile>,
}

/// A validated package.
#[derive(Clone, Debug)]
pub struct Package {
    /// The manifest.
    pub manifest: PluginManifest,
    /// Effects, in manifest order.
    pub effects: Vec<EffectPackage>,
    /// Warnings (errors make [`Package::load`] fail).
    pub warnings: Vec<Diagnostic>,
    /// Where it was loaded from.
    pub location: String,
    /// The package directory, for packages on disk.
    pub root: Option<PathBuf>,
}

impl Package {
    /// Loads and checks a package. On failure every diagnostic found is returned
    /// (manifest problems first, then shader problems).
    pub fn load(source: &dyn PackageSource) -> Result<Package, Vec<Diagnostic>> {
        let text = source.read_text(MANIFEST_FILE).map_err(|e| {
            vec![Diagnostic::error(
                MANIFEST_FILE,
                format!("cannot read the manifest in {}: {e}", source.describe()),
            )]
        })?;
        let manifest = PluginManifest::parse(&text).map_err(|e| vec![e.0])?;
        let mut diags = manifest.validate();
        let libraries: Vec<&str> = manifest
            .native
            .iter()
            .flat_map(|n| n.libraries.values().map(String::as_str))
            .collect();
        for path in manifest.referenced_files() {
            if crate::id::check_package_path(path).is_err() || source.exists(path) {
                continue;
            }
            if libraries.contains(&path) {
                // Build outputs: a source checkout has none; loading the native
                // code on this platform reports it if it matters.
                diags.push(Diagnostic::warning(
                    MANIFEST_FILE,
                    format!("native library `{path}` has not been built"),
                ));
            } else {
                diags.push(Diagnostic::error(
                    MANIFEST_FILE,
                    format!("file `{path}` does not exist in the package"),
                ));
            }
        }
        if has_errors(&diags) {
            return Err(diags);
        }

        let mut effects = Vec::with_capacity(manifest.effects.len());
        for decl in &manifest.effects {
            let def = effect_def(&manifest.plugin, decl);
            let mut shader_files = Vec::new();
            if decl.implementation == EffectImplementation::Wgsl {
                match read_shader_files(source, decl) {
                    Ok(files) => shader_files = files,
                    Err(d) => {
                        diags.extend(d);
                        continue;
                    }
                }
                if let Err(d) = wgsl::compile_effect(decl, &shader_files, WorkingFormat::default())
                {
                    let ctx = format!("effect \"{}\"", decl.id);
                    diags.extend(d.into_iter().map(|d| {
                        if d.context.is_empty() {
                            d.in_context(ctx.clone())
                        } else {
                            d
                        }
                    }));
                    continue;
                }
            }
            effects.push(EffectPackage {
                decl: decl.clone(),
                def,
                shader_files,
            });
        }
        if has_errors(&diags) {
            return Err(diags);
        }
        Ok(Package {
            manifest,
            effects,
            warnings: diags,
            location: source.describe(),
            root: source.root().map(Path::to_owned),
        })
    }

    /// The plugin id.
    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    /// The plugin version.
    pub fn version(&self) -> &semver::Version {
        &self.manifest.plugin.version
    }
}

/// Reads the includes and the shader of a declarative effect.
pub fn read_shader_files(
    source: &dyn PackageSource,
    decl: &EffectDecl,
) -> Result<Vec<SourceFile>, Vec<Diagnostic>> {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let paths = decl.includes.iter().chain(decl.shader.as_ref());
    for path in paths {
        if path.starts_with("plugin:") {
            errors.push(
                Diagnostic::error(
                    MANIFEST_FILE,
                    format!("include `{path}`: WGSL modules exported by other plugins are not supported by plugin API 1.0 yet"),
                )
                .in_context(format!("effect \"{}\"", decl.id)),
            );
            continue;
        }
        match source.read_text(path) {
            Ok(text) => files.push(SourceFile {
                path: path.clone(),
                text,
            }),
            Err(e) => errors.push(Diagnostic::error(path.clone(), format!("cannot read: {e}"))),
        }
    }
    if errors.is_empty() {
        Ok(files)
    } else {
        Err(errors)
    }
}
