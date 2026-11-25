//! # bae-effects — BAE's built-in effects (workstream 07)
//!
//! After Effects compatible effects on the GPU. Each effect keeps AE's
//! matchName as its document identity and AE's parameter disk ids, names,
//! order, defaults and ranges (`docs/spec/ae/_parts/effects--fx-*.md`), so AE
//! projects map 1:1; the mapping table for the AE importer (15) is
//! `docs/interop/ae-effects.md`.
//!
//! Built-ins go through the plugin interfaces (plugin spec §19), like any
//! third-party effect:
//!
//! * **Declarative WGSL packages** embedded in the binary
//!   (`packages/<id>/bae-plugin.toml` + shaders + `locales/zh-CN.toml`),
//!   loaded with `bae_plugin::DeclarativeEffect::load_builtin`: colour
//!   corrections, generators, transitions — pixel-style ones fuse with
//!   neighbouring pixel effects.
//! * **Rust `bae_render::Effect`s** where the pass structure depends on the
//!   parameters (blur pyramids, iterations) or the effect needs data the
//!   declarative format cannot express ([`effects`]).
//!
//! ```ignore
//! let mut defs = bae_model::EffectRegistry::new();
//! let mut library = bae_render::EffectLibrary::new();
//! bae_effects::register(&mut defs, &mut library);
//! bae_effects::prewarm(&gpu, wgpu::TextureFormat::Rgba16Float)?; // off the UI thread
//! ```
//!
//! [`catalog`] gives names, categories and keywords (English and Chinese) for
//! the Effects & Presets panel. [`Standalone`] renders an effect without the
//! render graph (tests, benchmarks, tools).

#![warn(missing_docs)]

mod blur;
pub mod catalog;
mod categories;
mod common;
pub mod effects;
mod gpu;
mod module;
mod obsolete;
mod params;
pub mod profile;
mod target;

use std::sync::{Arc, OnceLock};

use bae_gpu::{GpuContext, wgpu};
use bae_model::{EffectDef, EffectRegistry};
use bae_plugin::declarative::DeclarativeEffect;
use bae_plugin::manifest::{ParamType, ShaderStyle};
use bae_render::{Effect, EffectLibrary, EffectParams, GpuImage, IRect, RenderError};

pub use catalog::{EffectInfo, Form, category_zh};
pub use effects::blur::BLURRINESS_TO_SIGMA;
pub use module::EffectsModule;
pub use target::{EffectTarget, IMAGE_USAGE, Standalone, TargetEffect};

/// Plugin id of the built-in effects (`bae.effects/<id>`).
pub const PLUGIN_ID: &str = "bae.effects";

/// The implementation of a built-in effect.
#[derive(Clone)]
pub enum Implementation {
    /// An embedded declarative package (run by `bae-plugin`).
    Declarative(Arc<DeclarativeEffect>),
    /// A Rust effect.
    Rust(Arc<dyn TargetEffect>),
}

impl std::fmt::Debug for Implementation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Implementation::Declarative(_) => f.write_str("Declarative"),
            Implementation::Rust(_) => f.write_str("Rust"),
        }
    }
}

/// A built-in effect: catalog entry and implementation.
#[derive(Clone, Debug)]
pub struct Builtin {
    /// Catalog metadata.
    pub info: EffectInfo,
    /// Implementation.
    pub implementation: Implementation,
    alias: Option<Arc<obsolete::Alias>>,
}

impl Builtin {
    /// The effect as the render graph sees it.
    pub fn effect(&self) -> Arc<dyn Effect> {
        if let Some(alias) = &self.alias {
            return alias.clone();
        }
        match &self.implementation {
            Implementation::Declarative(fx) => fx.clone(),
            Implementation::Rust(fx) => {
                let fx: Arc<dyn TargetEffect> = fx.clone();
                Arc::new(Forward(fx))
            }
        }
    }

    /// The parameter definition.
    pub fn def(&self) -> &EffectDef {
        if let Some(alias) = &self.alias {
            return &alias.def;
        }
        match &self.implementation {
            Implementation::Declarative(fx) => fx.def(),
            Implementation::Rust(fx) => fx.def(),
        }
    }

    /// Renders into a [`Standalone`] target (tests, benchmarks, tools).
    pub fn render(
        &self,
        target: &mut Standalone,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let mut render = |params: &EffectParams<'_>| match &self.implementation {
            Implementation::Declarative(fx) => {
                target.render_declarative(fx, params, input, output_roi)
            }
            Implementation::Rust(fx) => fx.encode(target, params, input, output_roi),
        };
        if let Some(alias) = &self.alias {
            alias.with_params(params, render)
        } else {
            render(params)
        }
    }
}

/// `Arc<dyn TargetEffect>` as `Arc<dyn Effect>` (no trait upcasting needed).
struct Forward(Arc<dyn TargetEffect>);

impl Effect for Forward {
    fn def(&self) -> &EffectDef {
        self.0.def()
    }

    fn renders_pixels(&self) -> bool {
        self.0.renders_pixels()
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        self.0.output_bounds(params, input)
    }

    fn input_roi(&self, params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        self.0.input_roi(params, output_roi)
    }

    fn layer_dependencies(
        &self,
        params: &EffectParams<'_>,
    ) -> Vec<bae_render::effect::LayerDependency> {
        self.0.layer_dependencies(params)
    }

    fn mask_dependencies(&self, params: &EffectParams<'_>) -> Vec<bae_render::effect::MaskSelect> {
        self.0.mask_dependencies(params)
    }

    fn render(
        &self,
        ctx: &mut bae_render::EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.0.encode(ctx, params, input, output_roi)
    }
}

type Prewarm = fn(&gpu::Runtime, wgpu::TextureFormat) -> Result<(), RenderError>;

#[derive(Default)]
struct Registry {
    effects: Vec<Builtin>,
    prewarm: Vec<Prewarm>,
}

fn registry() -> &'static Registry {
    static BUILTINS: OnceLock<Registry> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let mut r = Registry::default();
        categories::blur_sharpen::register(&mut r);
        categories::channel::register(&mut r);
        categories::color::register(&mut r);
        categories::controls::register(&mut r);
        categories::distort::register(&mut r);
        categories::generate::register(&mut r);
        categories::keying::register(&mut r);
        categories::matte::register(&mut r);
        categories::noise_grain::register(&mut r);
        categories::perspective::register(&mut r);
        categories::stylize::register(&mut r);
        categories::time::register(&mut r);
        categories::transition::register(&mut r);
        categories::utility::register(&mut r);
        obsolete::register(&mut r);
        r.effects.sort_by(|a, b| {
            (a.info.category.as_str(), a.info.name.as_str())
                .cmp(&(b.info.category.as_str(), b.info.name.as_str()))
        });
        r
    })
}

/// Every built-in effect (loaded once, on first use).
pub fn builtins() -> &'static [Builtin] {
    &registry().effects
}

/// The built-in effect with document identity `match_name`.
pub fn get(match_name: &str) -> Option<&'static Builtin> {
    builtins().iter().find(|b| b.info.match_name == match_name)
}

/// Catalog entries of every built-in effect.
pub fn catalog() -> impl Iterator<Item = &'static EffectInfo> {
    builtins().iter().map(|b| &b.info)
}

/// Registers every built-in effect: definitions into `defs` (UI, agents, AE
/// import) and implementations into `library` (render graph). Returns the
/// number of effects.
pub fn register(defs: &mut EffectRegistry, library: &mut EffectLibrary) -> usize {
    for b in builtins() {
        defs.register(b.def().clone());
        library.register(b.effect());
    }
    builtins().len()
}

/// Compiles every pipeline for working format `format` ahead of the first
/// frame (call on a worker thread when a project opens).
pub fn prewarm(gpu: &Arc<GpuContext>, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    let rt = gpu::runtime(gpu);
    for warm in &registry().prewarm {
        warm(&rt, format)?;
    }
    for b in builtins() {
        if b.alias.is_none()
            && let Implementation::Declarative(fx) = &b.implementation
        {
            fx.prewarm(gpu, format)?;
        }
    }
    Ok(())
}

/// Catalog entry of a Rust effect (implementation and Chinese param names).
fn rust_builtin(
    (fx, names_zh): (Arc<dyn TargetEffect>, Vec<(u32, String)>),
    id: &str,
    name_zh: &str,
    keywords: &[&str],
) -> Builtin {
    let def = fx.def();
    Builtin {
        info: EffectInfo {
            match_name: def.match_name.clone(),
            full_id: format!("{PLUGIN_ID}/{id}"),
            name: def.name.clone(),
            name_zh: name_zh.to_owned(),
            category: def.category.clone(),
            keywords: keywords.iter().map(|s| (*s).to_owned()).collect(),
            form: Form::Rust,
            obsolete: def.category == "Obsolete",
            param_names_zh: names_zh,
            param_options_zh: rust_options_zh(def),
        },
        implementation: Implementation::Rust(fx),
        alias: None,
    }
}

/// Chinese popup options of the Rust effects (AE 26 zh_CN): each effect
/// module's `options_zh` table, then the blurs' Blur Dimensions popup.
fn rust_options_zh(def: &EffectDef) -> Vec<(u32, Vec<String>)> {
    const DIMENSIONS_ZH: &[&str] = &["水平和垂直", "水平", "垂直"];
    def.params
        .iter()
        .filter_map(|p| {
            let m = def.match_name.as_str();
            let zh = catalog::options_zh(m, p.id)
                .or_else(|| effects::options_zh(m, p.id))
                .or_else(|| effects::spatial::options_zh(m, p.id))
                .or_else(|| effects::distort::options_zh(m, p.id))
                .or((p.name == "Blur Dimensions").then_some(DIMENSIONS_ZH))?;
            Some((p.id, zh.iter().map(|s| (*s).to_owned()).collect()))
        })
        .collect()
}

fn declarative_info(fx: &DeclarativeEffect, zh: &toml::Table) -> EffectInfo {
    let decl = fx.decl();
    let def = fx.def();
    let entry = zh
        .get("effect")
        .and_then(|e| e.get(&decl.id))
        .and_then(toml::Value::as_table);
    let text = |k: &str| entry.and_then(|e| e.get(k)).and_then(toml::Value::as_str);
    let mut keywords = decl.keywords.clone();
    if let Some(list) = entry
        .and_then(|e| e.get("keywords"))
        .and_then(toml::Value::as_array)
    {
        keywords.extend(
            list.iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned),
        );
    }
    let names = entry
        .and_then(|e| e.get("params"))
        .and_then(toml::Value::as_table);
    let param_names_zh = decl
        .params
        .iter()
        .filter(|p| p.ty != ParamType::Group || names.is_some())
        .filter_map(|p| {
            let n = names?.get(&p.key)?.as_str()?;
            Some((p.id, n.to_owned()))
        })
        .collect();
    EffectInfo {
        match_name: def.match_name.clone(),
        full_id: format!("{PLUGIN_ID}/{}", decl.id),
        name: def.name.clone(),
        name_zh: text("name").unwrap_or(&def.name).to_owned(),
        category: def.category.clone(),
        keywords,
        form: match decl.shader_style() {
            ShaderStyle::Pixel => Form::DeclarativePixel,
            ShaderStyle::Compute => Form::DeclarativeCompute,
        },
        obsolete: def.category == "Obsolete",
        param_names_zh,
        param_options_zh: choices_zh(decl, entry),
    }
}

/// `[effect.<id>.choices]`: per parameter key either an array of options
/// (`channel = ["RGB", "红色", …]`) or a table by 1-based index (`1 = "…"`),
/// as option lists by parameter id.
fn choices_zh(
    decl: &bae_plugin::manifest::EffectDecl,
    entry: Option<&toml::Table>,
) -> Vec<(u32, Vec<String>)> {
    let Some(choices) = entry
        .and_then(|e| e.get("choices"))
        .and_then(toml::Value::as_table)
    else {
        return Vec::new();
    };
    decl.params
        .iter()
        .filter_map(|p| match choices.get(&p.key)? {
            toml::Value::Array(list) => Some((
                p.id,
                list.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            )),
            toml::Value::Table(table) => {
                let mut items: Vec<(u32, String)> = table
                    .iter()
                    .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_owned())))
                    .collect();
                items.sort_by_key(|(i, _)| *i);
                Some((p.id, items.into_iter().map(|(_, s)| s).collect()))
            }
            _ => None,
        })
        .collect()
}
