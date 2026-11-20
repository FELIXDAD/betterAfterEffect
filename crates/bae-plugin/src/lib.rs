//! # bae-plugin — the BAE plugin host
//!
//! Normative design: `docs/spec/plugin-architecture.md` ("spec §N"); user
//! documentation (Chinese): `docs/plugins/`.
//!
//! | module | contents | spec |
//! |---|---|---|
//! | [`manifest`] | `bae-plugin.toml` types, parsing, validation | §5 |
//! | [`id`] | plugin / contribution / param identifier rules, platform keys | §5.2 |
//! | [`effect_def`] | manifest params → `bae_model::EffectDef` | §5.3.7, §9.4 |
//! | [`expr`] | ROI and `identity_when` expressions | §11.7 |
//! | [`wgsl`] | declarative WGSL interface: prelude, assembly, naga validation, uniform layouts | §11 |
//! | [`package`] | loading and checking a package from disk or memory | §5.1, §5.4 |
//! | [`declarative`] | `DeclarativeEffect`: declarative effects on the GPU via `bae_render::Effect` | §11 |
//! | [`host`] | `PluginHost`: discovery, version selection, registration, runtime unload | §6, §7 |
//! | [`diag`] | diagnostics (`file:line:column: error: …`) | §16 |
//!
//! ## How an effect plugin reaches the renderer
//!
//! ```text
//! bae-plugin.toml + shaders ──Package::load──► EffectPackage { decl, def: EffectDef, shader_files }
//!     def  ──► bae_model::EffectRegistry::register      (UI, agents, AE import, placeholders)
//!     impl ──► bae_render::EffectLibrary::register      (render graph; the same trait as 07's built-ins)
//! ```
//!
//! Effect `match_name` = `<plugin-id>/<effect-id>`; unloading a plugin removes
//! its definitions with `EffectRegistry::unregister_plugin` and its
//! implementations from the library, and live instances become placeholders
//! (spec §20) without any document change.

#![warn(missing_docs)]

pub mod declarative;
pub mod diag;
pub mod effect_def;
pub mod expr;
pub mod host;
pub mod id;
pub mod manifest;
pub mod package;
pub mod wgsl;

pub use bae_plugin_sdk::{ApiVersion, ContributionKind, Implementation};
pub use declarative::DeclarativeEffect;
pub use diag::{Diagnostic, Severity};
pub use host::{LoadedPlugin, PluginDir, PluginHost, Scope};
pub use manifest::{EffectDecl, ParamDecl, PluginManifest};
pub use package::{DirSource, EffectPackage, MemorySource, Package, PackageSource};
