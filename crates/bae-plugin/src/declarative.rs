//! Declarative WGSL effects on the GPU (spec §11): [`DeclarativeEffect`]
//! implements `bae_render::Effect`, the same trait as the built-in effects of
//! 07, so the render graph runs plugin effects and built-ins the same way.
//!
//! Per frame the effect evaluates its parameters (no allocation beyond the
//! uniform buffers), decides identity and ROI from the manifest expressions,
//! and records one compute dispatch per pass into the frame's encoder.
//! Pipelines are compiled once per working format and cached in the effect;
//! they are freed when the last `Arc` of the effect is dropped (plugin unload).

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Weak};

use bae_gpu::GpuContext;
use bae_gpu::wgpu::{self, util::DeviceExt as _};
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, ImageAlpha, RenderError};
use parking_lot::Mutex;

use crate::diag::Diagnostic;
use crate::expr::{Expr, ExprEnv};
use crate::manifest::{
    EffectClass, EffectDecl, ParamType, PluginInfo, RoiExtent, ShaderStyle, TextureFormat,
    TextureRect,
};
use crate::package::{EffectPackage, Package, PackageSource};
use crate::wgsl::{self, Source, SourceFile, UniformLayout, WorkingFormat};

/// One numeric parameter as the effect packs it.
#[derive(Clone, Debug)]
struct Slot {
    id: u32,
    ty: ParamType,
    default: [f64; 4],
    relative: bool,
    range: Option<[f64; 2]>,
    choices: usize,
}

/// A declarative WGSL effect (manifest + shaders), ready to render.
pub struct DeclarativeEffect {
    def: EffectDef,
    decl: EffectDecl,
    files: Vec<SourceFile>,
    slots: Vec<Slot>,
    /// Index into `slots` for every scalar key, in [`Expr::parse`] key order.
    scalar_slots: Vec<usize>,
    identity: Option<Expr>,
    /// Read margins `[left, top, right, bottom]`.
    margins: [Option<Expr>; 4],
    extent: RoiExtent,
    grows: bool,
    sources: Vec<Source>,
    programs: Mutex<Vec<CachedProgram>>,
}

struct CachedProgram {
    gpu: Weak<GpuContext>,
    format: wgpu::TextureFormat,
    program: Arc<GpuProgram>,
}

impl std::fmt::Debug for DeclarativeEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclarativeEffect")
            .field("match_name", &self.def.match_name)
            .finish_non_exhaustive()
    }
}

impl DeclarativeEffect {
    /// Builds the effect from a validated package entry. Shaders were checked
    /// by [`Package::load`]; GPU pipelines are created on first use (or
    /// [`DeclarativeEffect::prewarm`]).
    pub fn new(plugin: &PluginInfo, effect: &EffectPackage) -> Result<Self, Vec<Diagnostic>> {
        let decl = effect.decl.clone();
        let ctx = format!("effect \"{}\"", decl.id);
        let err = |m: String| {
            vec![Diagnostic::error(crate::manifest::MANIFEST_FILE, m).in_context(ctx.clone())]
        };
        if decl.implementation != crate::manifest::EffectImplementation::Wgsl {
            return Err(err("not a declarative (wgsl) effect".into()));
        }
        let _ = plugin;
        let slots: Vec<Slot> = decl
            .params
            .iter()
            .filter(|p| p.ty.wgsl_type().is_some())
            .map(|p| {
                let n = p.default_numbers();
                let mut default = [0.0; 4];
                default[..n.len().min(4)].copy_from_slice(&n[..n.len().min(4)]);
                Slot {
                    id: p.id,
                    ty: p.ty,
                    default,
                    relative: p.is_relative_point(),
                    range: matches!(p.ty, ParamType::Float | ParamType::Int)
                        .then(|| p.valid_range()),
                    choices: p.choices.as_ref().map_or(0, Vec::len),
                }
            })
            .collect();
        let scalar_keys: Vec<&str> = decl
            .params
            .iter()
            .filter(|p| p.ty.is_scalar())
            .map(|p| p.key.as_str())
            .collect();
        let scalar_slots = decl
            .params
            .iter()
            .filter(|p| p.ty.is_scalar())
            .map(|p| slots.iter().position(|s| s.id == p.id).unwrap_or(0))
            .collect();
        let parse =
            |src: &str| Expr::parse(src, &scalar_keys).map_err(|e| err(format!("`{src}`: {e}")));
        let identity = decl.identity_when.as_deref().map(parse).transpose()?;
        let (margins, extent, grows) = match &decl.roi {
            Some(roi) => {
                let [l, t, r, b] = roi.margin_sources();
                (
                    [
                        l.map(parse).transpose()?,
                        t.map(parse).transpose()?,
                        r.map(parse).transpose()?,
                        b.map(parse).transpose()?,
                    ],
                    roi.extent,
                    roi.grows,
                )
            }
            None => ([None, None, None, None], None, false),
        };
        let extent = extent.unwrap_or(match decl.class {
            EffectClass::Generator => RoiExtent::Layer,
            _ => RoiExtent::Input,
        });
        Ok(Self {
            def: effect.def.clone(),
            sources: wgsl::sources(&decl),
            decl,
            files: effect.shader_files.clone(),
            slots,
            scalar_slots,
            identity,
            margins,
            extent,
            grows,
            programs: Mutex::new(Vec::new()),
        })
    }

    /// All declarative effects of a loaded package.
    pub fn from_package(package: &Package) -> Result<Vec<Arc<Self>>, Vec<Diagnostic>> {
        package
            .effects
            .iter()
            .filter(|e| e.decl.implementation == crate::manifest::EffectImplementation::Wgsl)
            .map(|e| Self::new(&package.manifest.plugin, e).map(Arc::new))
            .collect()
    }

    /// Built-in effects authored as declarative packages embedded in the
    /// binary (spec §19), e.g. `MemorySource::new("builtin:bae.effects")
    /// .with_file("bae-plugin.toml", include_str!(...))`.
    ///
    /// The package id must be first-party (`bae.*`). The definitions are those
    /// of built-ins: `plugin` is `None` (so `EffectRegistry::unregister_plugin`
    /// never removes them and projects never show them as missing), and the
    /// `match_name` is the effect's first `ae_match_names` entry when it has one
    /// (AE projects map 1:1), else `<plugin-id>/<effect-id>`.
    pub fn load_builtin(source: &dyn PackageSource) -> Result<Vec<Arc<Self>>, Vec<Diagnostic>> {
        let package = Package::load(source)?;
        if !crate::id::is_first_party(package.id()) {
            return Err(vec![Diagnostic::error(
                crate::manifest::MANIFEST_FILE,
                format!(
                    "built-in packages must use a first-party id (bae.*), not `{}`",
                    package.id()
                ),
            )]);
        }
        package
            .effects
            .iter()
            .filter(|e| e.decl.implementation == crate::manifest::EffectImplementation::Wgsl)
            .map(|e| {
                let mut fx = Self::new(&package.manifest.plugin, e)?;
                fx.def.plugin = None;
                if let Some(ae) = e.decl.ae_match_names.first() {
                    fx.def.match_name = ae.clone();
                    for param in &mut fx.def.params {
                        param.match_name = Some(format!("{ae}-{:04}", param.id));
                    }
                }
                Ok(Arc::new(fx))
            })
            .collect()
    }

    /// The manifest declaration.
    pub fn decl(&self) -> &EffectDecl {
        &self.decl
    }

    /// Compiles the pipelines for `format` ahead of the first frame.
    pub fn prewarm(
        &self,
        gpu: &Arc<GpuContext>,
        format: wgpu::TextureFormat,
    ) -> Result<(), RenderError> {
        self.program(gpu, format).map(|_| ())
    }

    /// Number of compiled pipeline sets (one per working format used).
    pub fn compiled_formats(&self) -> usize {
        self.programs.lock().len()
    }

    fn program(
        &self,
        gpu: &Arc<GpuContext>,
        format: wgpu::TextureFormat,
    ) -> Result<Arc<GpuProgram>, RenderError> {
        let device = Arc::downgrade(gpu);
        let mut programs = self.programs.lock();
        programs.retain(|entry| entry.gpu.strong_count() > 0);
        if let Some(entry) = programs
            .iter()
            .find(|p| p.gpu.ptr_eq(&device) && p.format == format)
        {
            return Ok(entry.program.clone());
        }
        let program = Arc::new(GpuProgram::build(self, gpu, format)?);
        programs.push(CachedProgram {
            gpu: device,
            format,
            program: program.clone(),
        });
        Ok(program)
    }

    /// Evaluates every numeric parameter at the frame (render-pixel units).
    fn evaluate(&self, params: &EffectParams<'_>, layer: IRect) -> Values {
        let scale = params.scale;
        let layer_size = [
            f64::from(layer.width()) / scale.max(1e-9),
            f64::from(layer.height()) / scale.max(1e-9),
        ];
        let mut slots = Vec::with_capacity(self.slots.len());
        for s in &self.slots {
            let v = params.value(s.id);
            let num = |v: Option<&Value>| match v {
                Some(Value::Float(x)) => Some(*x),
                Some(Value::Int(i)) => Some(*i as f64),
                Some(Value::Bool(b)) => Some(f64::from(u8::from(*b))),
                _ => None,
            };
            let clamp = |x: f64| match s.range {
                Some([lo, hi]) => x.clamp(lo, hi),
                None => x,
            };
            let x = match s.ty {
                ParamType::Float | ParamType::Angle => {
                    [clamp(num(v).unwrap_or(s.default[0])), 0.0, 0.0, 0.0]
                }
                ParamType::Int => [clamp(num(v).unwrap_or(s.default[0]).round()), 0.0, 0.0, 0.0],
                ParamType::Checkbox => [
                    f64::from(u8::from(num(v).unwrap_or(s.default[0]) != 0.0)),
                    0.0,
                    0.0,
                    0.0,
                ],
                ParamType::Popup => {
                    let i = num(v).unwrap_or(s.default[0]).round();
                    [i.clamp(1.0, s.choices.max(1) as f64), 0.0, 0.0, 0.0]
                }
                ParamType::Color => match v {
                    Some(Value::Color(c)) => *c,
                    _ => s.default,
                },
                ParamType::Point | ParamType::Point3d => {
                    let p = match v {
                        Some(Value::Vec2([x, y])) => [*x, *y, 0.0],
                        Some(Value::Vec3([x, y, z])) => [*x, *y, *z],
                        _ if s.relative => [
                            s.default[0] * layer_size[0],
                            s.default[1] * layer_size[1],
                            s.default[2],
                        ],
                        _ => [s.default[0], s.default[1], s.default[2]],
                    };
                    // Layer pixels → render pixels.
                    [p[0] * scale, p[1] * scale, p[2] * scale, 0.0]
                }
                _ => [0.0; 4],
            };
            slots.push(x);
        }
        Values { slots, scale }
    }

    fn env<'a>(&'a self, values: &'a Values) -> Env<'a> {
        Env {
            effect: self,
            values,
        }
    }

    fn is_identity(&self, values: &Values) -> bool {
        self.identity
            .as_ref()
            .is_some_and(|e| e.is_true(&self.env(values)))
    }

    /// Read margins `[left, top, right, bottom]` in render pixels.
    fn margins(&self, values: &Values) -> [i32; 4] {
        let env = self.env(values);
        self.margins.each_ref().map(|m| match m {
            Some(e) => (e.eval(&env).max(0.0) * values.scale).ceil().min(1e6) as i32,
            None => 0,
        })
    }

    /// The rects of a frame: output and intermediates.
    fn plan(&self, values: &Values, output_roi: IRect) -> Plan {
        let [l, t, r, b] = self.margins(values);
        let input_rect = IRect::new(
            output_roi.x0 - l,
            output_roi.y0 - t,
            output_roi.x1 + r,
            output_roi.y1 + b,
        );
        let textures = if self.decl.shader_style() == ShaderStyle::Compute {
            self.decl
                .textures
                .iter()
                .map(|t| match t.rect {
                    TextureRect::Output => output_roi,
                    TextureRect::Input => input_rect,
                })
                .collect()
        } else {
            Vec::new()
        };
        Plan {
            output: output_roi,
            textures,
        }
    }

    /// Records the passes. `targets` are the output image and one image per
    /// intermediate texture (in manifest order), already allocated with the
    /// rects of [`Plan`].
    #[allow(clippy::too_many_arguments)]
    fn encode(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        program: &GpuProgram,
        values: &Values,
        frame: &FrameInfo,
        input: &GpuImage,
        output: &GpuImage,
        textures: &[GpuImage],
    ) {
        let device = gpu.device();
        // Params and sources: one buffer each, shared by every pass.
        let params_buffer = program.params.as_ref().map(|layout| {
            let mut bytes = vec![0u8; layout.buffer_size() as usize];
            for ((slot, v), &offset) in self
                .slots
                .iter()
                .zip(&values.slots)
                .zip(&program.param_offsets)
            {
                pack_param(&mut bytes, offset as usize, slot.ty, v);
            }
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("bae-plugin params"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            })
        });
        let mut src_bytes = vec![0u8; program.sources.buffer_size() as usize];
        for (i, src) in self.sources.iter().enumerate() {
            let image = match src {
                Source::Input => Some(input),
                Source::Texture(name) => self
                    .decl
                    .textures
                    .iter()
                    .position(|t| &t.name == name)
                    .map(|k| &textures[k]),
                Source::To | Source::LayerParam(_) => None,
            };
            if let Some(image) = image {
                let o = program.source_offsets[i] as usize;
                let rect = render_rect(image.rect, frame.pixel_scale);
                put_i32s(&mut src_bytes, o, &[rect.x0, rect.y0, rect.x1, rect.y1]);
                let map = [
                    f64::from(image.rect.x0) / frame.pixel_scale,
                    f64::from(image.rect.y0) / frame.pixel_scale,
                    frame.pixel_scale / image.pixel_scale,
                ];
                for (k, x) in map.into_iter().enumerate() {
                    src_bytes[o + 16 + 4 * k..o + 20 + 4 * k]
                        .copy_from_slice(&(x as f32).to_le_bytes());
                }
                let [width, height] = image.texel_size();
                put_i32s(&mut src_bytes, o + 32, &[width as i32, height as i32]);
            }
        }
        let sources_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bae-plugin sources"),
            contents: &src_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(self.def.match_name.as_str()),
            timestamp_writes: None,
        });
        for (index, pass) in program.passes.iter().enumerate() {
            let target = match pass.target {
                PassTarget::Output => output,
                PassTarget::Texture(k) => &textures[k],
            };
            let target_rect = render_rect(target.rect, frame.pixel_scale);
            let frame_bytes = frame.pack(&program.frame, target_rect, values.scale, index as u32);
            let frame_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("bae-plugin frame"),
                contents: &frame_bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: wgsl::BINDING_FRAME,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: wgsl::BINDING_SOURCES,
                    resource: sources_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: wgsl::BINDING_OUT,
                    resource: wgpu::BindingResource::TextureView(&target.view),
                },
            ];
            if let Some(buffer) = &params_buffer {
                entries.push(wgpu::BindGroupEntry {
                    binding: wgsl::BINDING_PARAMS,
                    resource: buffer.as_entire_binding(),
                });
            }
            for (i, src) in self.sources.iter().enumerate() {
                let view = match src {
                    Source::Input => &input.view,
                    Source::Texture(name) => {
                        let k = self.decl.textures.iter().position(|t| &t.name == name);
                        match k {
                            Some(k) if pass.target != PassTarget::Texture(k) => &textures[k].view,
                            _ => &program.placeholder,
                        }
                    }
                    Source::To | Source::LayerParam(_) => &program.placeholder,
                };
                entries.push(wgpu::BindGroupEntry {
                    binding: wgsl::BINDING_FIRST_SOURCE + i as u32,
                    resource: wgpu::BindingResource::TextureView(view),
                });
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bae-plugin pass"),
                layout: &pass.layout,
                entries: &entries,
            });
            let [w, h] = target_rect.size();
            if w == 0 || h == 0 {
                continue;
            }
            cpass.set_pipeline(&pass.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(
                w.div_ceil(pass.workgroup[0]),
                h.div_ceil(pass.workgroup[1]),
                1,
            );
        }
    }

    /// Renders with explicit GPU objects instead of an `EncodeContext`
    /// (tests, benchmarks and hosts without the render graph). `allocate`
    /// creates an image covering a full-resolution rectangle at a pixel scale
    /// in a format. `output_roi` is in render pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn render_with(
        &self,
        gpu: &Arc<GpuContext>,
        encoder: &mut wgpu::CommandEncoder,
        format: wgpu::TextureFormat,
        allocate: &mut dyn FnMut(IRect, f64, wgpu::TextureFormat) -> Result<GpuImage, RenderError>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let grid = pixel_scale(params, input);
        let layer = layer_rect(params, input, grid);
        let values = self.evaluate(params, layer);
        if self.is_identity(&values) {
            return Ok(input.clone());
        }
        let program = self.program(gpu, format)?;
        let plan = self.plan(&values, output_roi);
        let output = allocate(pixel_rect(plan.output, grid), grid, format)?;
        let mut textures = Vec::with_capacity(plan.textures.len());
        for (t, rect) in self.decl.textures.iter().zip(&plan.textures) {
            let f = texture_format(t.format, format);
            textures.push(allocate(pixel_rect(*rect, grid), grid, f)?);
        }
        let frame = FrameInfo::new(params, layer, grid, input.alpha);
        self.encode(
            gpu, encoder, &program, &values, &frame, input, &output, &textures,
        );
        Ok(output)
    }
}

impl Effect for DeclarativeEffect {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        let values = self.evaluate(params, input);
        if self.is_identity(&values) {
            return input;
        }
        match self.extent {
            // Layer bounds are not in EffectParams yet (R04-11): the input
            // extent is the layer's own extent for the first effect.
            RoiExtent::Input | RoiExtent::Layer | RoiExtent::Infinite => {
                if self.grows {
                    let [l, t, r, b] = self.margins(&values);
                    // Content read from the left spreads to the right, and so on.
                    IRect::new(input.x0 - r, input.y0 - b, input.x1 + l, input.y1 + t)
                } else {
                    input
                }
            }
        }
    }

    fn input_roi(&self, params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        let values = self.evaluate(params, output_roi);
        if self.is_identity(&values) {
            return output_roi;
        }
        let [l, t, r, b] = self.margins(&values);
        IRect::new(
            output_roi.x0 - l,
            output_roi.y0 - t,
            output_roi.x1 + r,
            output_roi.y1 + b,
        )
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let grid = pixel_scale(params, input);
        let layer = layer_rect(params, input, grid);
        let values = self.evaluate(params, layer);
        if self.is_identity(&values) {
            return Ok(input.clone());
        }
        let format = ctx.format;
        let gpu = ctx.gpu.clone();
        let program = self.program(&gpu, format)?;
        let plan = self.plan(&values, output_roi);
        let output = ctx.allocate(pixel_rect(plan.output, grid), grid)?;
        let mut textures = Vec::with_capacity(plan.textures.len());
        for (t, rect) in self.decl.textures.iter().zip(&plan.textures) {
            let f = texture_format(t.format, format);
            textures.push(if f == format {
                ctx.allocate(pixel_rect(*rect, grid), grid)?
            } else {
                create_image(&gpu, pixel_rect(*rect, grid), grid, f)?
            });
        }
        let frame = FrameInfo::new(params, layer, grid, input.alpha);
        self.encode(
            &gpu,
            ctx.encoder,
            &program,
            &values,
            &frame,
            input,
            &output,
            &textures,
        );
        Ok(output)
    }
}

/// Evaluated numeric params (render-pixel units for points).
struct Values {
    slots: Vec<[f64; 4]>,
    scale: f64,
}

struct Env<'a> {
    effect: &'a DeclarativeEffect,
    values: &'a Values,
}

impl ExprEnv for Env<'_> {
    fn param(&self, index: usize) -> f64 {
        self.effect
            .scalar_slots
            .get(index)
            .and_then(|&s| self.values.slots.get(s))
            .map_or(0.0, |v| v[0])
    }

    fn downsample(&self) -> [f64; 2] {
        [self.values.scale; 2]
    }
}

struct Plan {
    output: IRect,
    textures: Vec<IRect>,
}

fn pixel_scale(params: &EffectParams<'_>, input: &GpuImage) -> f64 {
    if params.layer_bounds.is_empty() {
        input.pixel_scale
    } else {
        1.0 / params.scale
    }
}

fn layer_rect(params: &EffectParams<'_>, input: &GpuImage, grid: f64) -> IRect {
    let rect = if params.layer_bounds.is_empty() {
        input.rect
    } else {
        params.layer_bounds
    };
    render_rect(rect, grid)
}

fn render_rect(r: IRect, scale: f64) -> IRect {
    let min = |value| (f64::from(value) / scale).floor() as i32;
    let max = |value| (f64::from(value) / scale).ceil() as i32;
    IRect::new(min(r.x0), min(r.y0), max(r.x1), max(r.y1))
}

fn pixel_rect(r: IRect, scale: f64) -> IRect {
    let px = |value| (f64::from(value) * scale).round() as i32;
    IRect::new(px(r.x0), px(r.y0), px(r.x1), px(r.y1))
}

/// Per-frame values of `BaeFrame` that do not depend on the pass.
struct FrameInfo {
    output_rect: IRect,
    layer_bounds: IRect,
    pixel_scale: f64,
    time: f32,
    frame_index: i32,
    frame_rate: f32,
    time_step: f32,
    seed: u32,
    flags: u32,
}

impl FrameInfo {
    fn new(params: &EffectParams<'_>, layer: IRect, pixel_scale: f64, alpha: ImageAlpha) -> Self {
        let mut h = rustc_hash::FxHasher::default();
        params.effect.path.hash(&mut h);
        Self {
            output_rect: IRect::EMPTY,
            layer_bounds: layer,
            pixel_scale,
            time: params.time.as_seconds() as f32,
            frame_index: params.frame_rate.time_to_frame(params.time) as i32,
            frame_rate: params.frame_rate.as_f64() as f32,
            time_step: params.time_step.as_seconds() as f32,
            seed: h.finish() as u32,
            flags: frame_flags(params)
                | match alpha {
                    ImageAlpha::Premultiplied => 0,
                    ImageAlpha::Straight => wgsl::FRAME_INPUT_STRAIGHT,
                    ImageAlpha::Opaque => wgsl::FRAME_INPUT_OPAQUE,
                },
        }
    }

    fn pack(&self, layout: &UniformLayout, target: IRect, scale: f64, pass: u32) -> Vec<u8> {
        let mut b = vec![0u8; layout.buffer_size() as usize];
        let output = if self.output_rect == IRect::EMPTY {
            target
        } else {
            self.output_rect
        };
        let mut rect = |name: &str, r: IRect| {
            if let Some(o) = layout.offset(name) {
                put_i32s(&mut b, o as usize, &[r.x0, r.y0, r.x1, r.y1]);
            }
        };
        rect("target_rect", target);
        rect("output_rect", output);
        rect("layer_bounds", self.layer_bounds);
        let mut f32s = |name: &str, v: &[f32]| {
            if let Some(o) = layout.offset(name) {
                for (k, x) in v.iter().enumerate() {
                    let o = o as usize + 4 * k;
                    b[o..o + 4].copy_from_slice(&x.to_le_bytes());
                }
            }
        };
        f32s("downsample", &[scale as f32, scale as f32]);
        f32s("time", &[self.time]);
        f32s("frame_rate", &[self.frame_rate]);
        f32s("time_step", &[self.time_step]);
        f32s("pixel_aspect", &[1.0]);
        let mut u32s = |name: &str, v: u32| {
            if let Some(o) = layout.offset(name) {
                b[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
            }
        };
        u32s("flags", self.flags);
        u32s("seed", self.seed);
        u32s("pass_index", pass);
        u32s("frame_index", self.frame_index as u32);
        b
    }
}

/// `BaeFrame.flags` for `params`.
fn frame_flags(params: &EffectParams<'_>) -> u32 {
    let mut flags = wgsl::FRAME_HIGH_QUALITY;
    if params.linear {
        flags |= wgsl::FRAME_LINEAR;
    }
    if params.purpose == bae_render::RenderPurpose::Final {
        flags |= wgsl::FRAME_FINAL;
    }
    if params.float {
        flags |= wgsl::FRAME_FLOAT;
    } else if params.sixteen_bpc {
        flags |= wgsl::FRAME_16_BPC;
    }
    flags
}

fn put_i32s(b: &mut [u8], offset: usize, v: &[i32]) {
    for (k, x) in v.iter().enumerate() {
        let o = offset + 4 * k;
        b[o..o + 4].copy_from_slice(&x.to_le_bytes());
    }
}

fn pack_param(b: &mut [u8], offset: usize, ty: ParamType, v: &[f64; 4]) {
    let mut put =
        |k: usize, bytes: [u8; 4]| b[offset + 4 * k..offset + 4 * k + 4].copy_from_slice(&bytes);
    match ty {
        ParamType::Float | ParamType::Angle => put(0, (v[0] as f32).to_le_bytes()),
        ParamType::Int => put(0, (v[0] as i32).to_le_bytes()),
        ParamType::Checkbox | ParamType::Popup => put(0, (v[0] as u32).to_le_bytes()),
        ParamType::Point => (0..2).for_each(|k| put(k, (v[k] as f32).to_le_bytes())),
        ParamType::Point3d => (0..3).for_each(|k| put(k, (v[k] as f32).to_le_bytes())),
        ParamType::Color => (0..4).for_each(|k| put(k, (v[k] as f32).to_le_bytes())),
        _ => {}
    }
}

fn texture_format(t: TextureFormat, working: wgpu::TextureFormat) -> wgpu::TextureFormat {
    match t {
        TextureFormat::Working => working,
        TextureFormat::Rgba16f => wgpu::TextureFormat::Rgba16Float,
        TextureFormat::Rgba32f => wgpu::TextureFormat::Rgba32Float,
        TextureFormat::R32f => wgpu::TextureFormat::R32Float,
        TextureFormat::Rg32f => wgpu::TextureFormat::Rg32Float,
        TextureFormat::Rgba8unorm => wgpu::TextureFormat::Rgba8Unorm,
    }
}

/// A transparent image with storage + sampling usage (intermediates in a
/// non-working format; the renderer's pool only hands out working images).
pub fn create_image(
    gpu: &GpuContext,
    rect: IRect,
    pixel_scale: f64,
    format: wgpu::TextureFormat,
) -> Result<GpuImage, RenderError> {
    let [w, h] = bae_render::source::texels_for(rect, pixel_scale);
    let limit = gpu.limits().max_texture_dimension_2d;
    if w > limit || h > limit {
        return Err(RenderError::TooLarge {
            width: w,
            height: h,
            limit,
        });
    }
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("bae-plugin image"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Through the constructor, not a struct literal: GpuImage gains private
    // fields (pool leases) in bae-render.
    let mut image = GpuImage::from_texture(texture, bae_render::source::ImageAlpha::Premultiplied);
    image.rect = rect;
    image.pixel_scale = pixel_scale;
    Ok(image)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassTarget {
    Output,
    Texture(usize),
}

struct PassProgram {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    workgroup: [u32; 2],
    target: PassTarget,
}

/// Pipelines and layouts of one effect for one working format.
struct GpuProgram {
    passes: Vec<PassProgram>,
    frame: UniformLayout,
    params: Option<UniformLayout>,
    param_offsets: Vec<u32>,
    sources: UniformLayout,
    source_offsets: Vec<u32>,
    placeholder: wgpu::TextureView,
}

impl GpuProgram {
    fn build(
        effect: &DeclarativeEffect,
        gpu: &GpuContext,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RenderError> {
        let working = match format {
            wgpu::TextureFormat::Rgba16Float => WorkingFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba32Float => WorkingFormat::Rgba32Float,
            other => {
                return Err(RenderError::Other(format!(
                    "{}: unsupported working format {other:?}",
                    effect.def.match_name
                )));
            }
        };
        let modules = wgsl::compile_effect(&effect.decl, &effect.files, working).map_err(|d| {
            let text: Vec<String> = d.iter().map(ToString::to_string).collect();
            RenderError::Other(format!("{}: {}", effect.def.match_name, text.join("; ")))
        })?;
        let first = &modules[0].module;
        let frame = wgsl::uniform_layout(first, "BaeFrame")
            .ok_or_else(|| RenderError::Other("prelude without BaeFrame".into()))?;
        let params = wgsl::uniform_layout(first, "BaeParams");
        let sources = wgsl::uniform_layout(first, "BaeSources")
            .ok_or_else(|| RenderError::Other("prelude without BaeSources".into()))?;
        let param_offsets = effect
            .decl
            .params
            .iter()
            .filter(|p| p.ty.wgsl_type().is_some())
            .map(|p| params.as_ref().and_then(|l| l.offset(&p.key)).unwrap_or(0))
            .collect();
        let source_offsets = effect
            .sources
            .iter()
            .map(|s| sources.offset(s.name()).unwrap_or(0))
            .collect();
        let device = gpu.device();
        let formats = wgsl::pass_target_formats(&effect.decl, working);

        let pixel = effect.decl.shader_style() == ShaderStyle::Pixel;
        let pass_specs: Vec<(String, PassTarget)> = if pixel {
            vec![(wgsl::PIXEL_KERNEL_ENTRY.to_owned(), PassTarget::Output)]
        } else {
            effect
                .decl
                .passes
                .iter()
                .map(|p| {
                    let target = match effect.decl.textures.iter().position(|t| t.name == p.writes)
                    {
                        Some(k) => PassTarget::Texture(k),
                        None => PassTarget::Output,
                    };
                    (p.entry.clone(), target)
                })
                .collect()
        };
        let mut passes = Vec::with_capacity(pass_specs.len());
        for (i, (entry, target)) in pass_specs.into_iter().enumerate() {
            let compiled = &modules[i.min(modules.len() - 1)];
            let target_format = match target {
                PassTarget::Output => format,
                PassTarget::Texture(k) => texture_format(effect.decl.textures[k].format, format),
            };
            debug_assert_eq!(formats.len(), modules.len());
            let workgroup = compiled
                .module
                .entry_points
                .iter()
                .find(|e| e.name == entry)
                .map_or([1, 1], |e| {
                    [e.workgroup_size[0].max(1), e.workgroup_size[1].max(1)]
                });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(effect.def.match_name.as_str()),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(compiled.assembled.source.clone())),
            });
            let uniform = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size),
                },
                count: None,
            };
            let mut entries = vec![
                uniform(wgsl::BINDING_FRAME, u64::from(frame.size)),
                uniform(wgsl::BINDING_SOURCES, u64::from(sources.size)),
                wgpu::BindGroupLayoutEntry {
                    binding: wgsl::BINDING_OUT,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: target_format,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ];
            if let Some(p) = &params {
                entries.push(uniform(wgsl::BINDING_PARAMS, u64::from(p.size)));
            }
            for k in 0..effect.sources.len() {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: wgsl::BINDING_FIRST_SOURCE + k as u32,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
            }
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bae-plugin pass"),
                entries: &entries,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("bae-plugin pass"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(effect.def.match_name.as_str()),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry.as_str()),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
            passes.push(PassProgram {
                pipeline,
                layout,
                workgroup,
                target,
            });
        }
        let placeholder = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-plugin placeholder"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            passes,
            frame,
            params,
            param_offsets,
            sources,
            source_offsets,
            placeholder,
        })
    }
}
