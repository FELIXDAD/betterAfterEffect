//! The declarative WGSL interface (spec §11): the generated prelude, module
//! assembly, validation with naga and the uniform layouts the host packs.
//!
//! A declarative effect's shader module is assembled from
//!
//! ```text
//! <prelude>    generated from the manifest: types, bindings, helpers   (virtual file)
//! <includes>   [effect] includes, in order
//! <shader>     [effect] shader
//! <kernel>     pixel style only: the generated @compute entry point     (virtual file)
//! ```
//!
//! One module is compiled per distinct pass-target format (pixel style: one).
//! Author code never declares bindings; everything it can touch is listed in
//! [`PRELUDE_DOC`]. Diagnostics point at `file:line:column` of the author files.
//!
//! The prelude is part of the plugin API: adding helpers or fields is a minor
//! API change, renaming or removing them a major one. The `BaeSources` struct
//! and binding numbers are host-internal (authors use the helpers).

use std::fmt::Write as _;

use naga::valid::{Capabilities, GlobalUse, ModuleInfo, ValidationFlags, Validator};

use crate::diag::Diagnostic;
use crate::manifest::{EffectClass, EffectDecl, ParamType, ShaderStyle, TextureFormat};

/// Name of the virtual prelude file in diagnostics.
pub const PRELUDE_FILE: &str = "<prelude>";
/// Name of the virtual pixel-kernel file in diagnostics.
pub const KERNEL_FILE: &str = "<kernel>";
/// Entry point of the generated pixel-style kernel.
pub const PIXEL_KERNEL_ENTRY: &str = "bae_pixel_main";
/// Workgroup size of the generated pixel-style kernel.
pub const PIXEL_WORKGROUP: [u32; 2] = [16, 16];
/// Maximum invocations per workgroup for author entry points.
pub const MAX_WORKGROUP_INVOCATIONS: u32 = 256;

/// Binding of the `frame` uniform (`BaeFrame`).
pub const BINDING_FRAME: u32 = 0;
/// Binding of the `params` uniform (`BaeParams`), present iff the effect has numeric params.
pub const BINDING_PARAMS: u32 = 1;
/// Binding of the source-rect uniform (`BaeSources`).
pub const BINDING_SOURCES: u32 = 2;
/// Binding of `bae_out`, the current pass target (write-only storage texture).
pub const BINDING_OUT: u32 = 3;
/// First binding of the sampled source textures (`bae_tex_<src>`, in [`sources`] order).
pub const BINDING_FIRST_SOURCE: u32 = 4;

/// `frame.flags`: the working space is linear light.
pub const FRAME_LINEAR: u32 = 1;
/// `frame.flags`: final render (render queue, `bae render`).
pub const FRAME_FINAL: u32 = 2;
/// `frame.flags`: high quality (not draft).
pub const FRAME_HIGH_QUALITY: u32 = 4;

/// Summary of what the prelude provides, for documentation and error messages.
pub const PRELUDE_DOC: &str = "\
frame: BaeFrame (target_rect, output_rect, layer_bounds, downsample, time, time_step, \
frame_rate, pixel_aspect, shutter_angle, shutter_phase, flags, seed, frame_index, pass_index); \
params: BaeParams (one field per numeric param); bae_load_<src>, bae_sample_<src>, \
bae_rect_<src>, bae_store, bae_target_pixel, bae_in_target, bae_scale, bae_to_layer, \
bae_from_layer, bae_layer_uv, bae_premultiply, bae_unpremultiply, bae_to_linear, \
bae_from_linear, bae_luma, bae_hash";

/// A readable source of a declarative effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The effect's own input (the layer, or the previous effect).
    Input,
    /// Transitions: the layer transitioned to.
    To,
    /// A layer parameter (by key).
    LayerParam(String),
    /// An intermediate texture (by name).
    Texture(String),
}

impl Source {
    /// The `<src>` part of `bae_load_<src>` / `bae_tex_<src>`.
    pub fn name(&self) -> &str {
        match self {
            Source::Input => "input",
            Source::To => "to",
            Source::LayerParam(k) | Source::Texture(k) => k,
        }
    }
}

/// Sources of an effect, in binding order: input, to (transitions), layer
/// params (definition order), intermediate textures (manifest order).
pub fn sources(effect: &EffectDecl) -> Vec<Source> {
    let mut out = vec![Source::Input];
    if effect.class == EffectClass::Transition {
        out.push(Source::To);
    }
    out.extend(
        effect
            .params
            .iter()
            .filter(|p| p.ty == ParamType::Layer)
            .map(|p| Source::LayerParam(p.key.clone())),
    );
    if effect.shader_style() == ShaderStyle::Compute {
        out.extend(
            effect
                .textures
                .iter()
                .map(|t| Source::Texture(t.name.clone())),
        );
    }
    out
}

/// The working texture format of a render (`bae_render::WorkFormat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WorkingFormat {
    /// RGBA16F (default).
    #[default]
    Rgba16Float,
    /// RGBA32F.
    Rgba32Float,
}

impl WorkingFormat {
    /// WGSL storage texel format.
    pub fn wgsl(self) -> &'static str {
        match self {
            WorkingFormat::Rgba16Float => "rgba16float",
            WorkingFormat::Rgba32Float => "rgba32float",
        }
    }

    /// Resolves an intermediate texture format.
    pub fn resolve(self, format: TextureFormat) -> &'static str {
        format.wgsl_storage_format().unwrap_or(self.wgsl())
    }
}

/// The storage format of each pass target (pixel style: one entry, `output`).
pub fn pass_target_formats(effect: &EffectDecl, working: WorkingFormat) -> Vec<&'static str> {
    if effect.shader_style() == ShaderStyle::Pixel {
        return vec![working.wgsl()];
    }
    effect
        .passes
        .iter()
        .map(|pass| {
            effect
                .textures
                .iter()
                .find(|t| t.name == pass.writes)
                .map_or(working.wgsl(), |t| working.resolve(t.format))
        })
        .collect()
}

/// Generates the prelude for `effect` with `bae_out` in `target_format`
/// (a WGSL storage texel format such as `rgba16float`).
pub fn prelude(effect: &EffectDecl, target_format: &str) -> String {
    let mut s = String::with_capacity(8 * 1024);
    let f16 = effect.features.iter().any(|f| f == "f16");
    if f16 {
        s.push_str("enable f16;\n");
    }
    s.push_str(
        "\
// ---- BAE declarative effect prelude (plugin API 0.1), generated by the host ----
// Rects are half-open [min, max) in render pixels (full-resolution layer pixels
// × downsample, origin at the layer's top-left, y down).
struct BaeRect { min: vec2<i32>, max: vec2<i32> }

struct BaeFrame {
    target_rect: BaeRect,
    output_rect: BaeRect,
    layer_bounds: BaeRect,
    downsample: vec2<f32>,
    time: f32,
    time_step: f32,
    frame_rate: f32,
    pixel_aspect: f32,
    shutter_angle: f32,
    shutter_phase: f32,
    flags: u32,
    seed: u32,
    frame_index: i32,
    pass_index: u32,
}

const BAE_FRAME_LINEAR: u32 = 1u;
const BAE_FRAME_FINAL: u32 = 2u;
const BAE_FRAME_HIGH_QUALITY: u32 = 4u;

@group(0) @binding(0) var<uniform> frame: BaeFrame;
",
    );

    // Params.
    let fields: Vec<(&str, &str)> = effect
        .params
        .iter()
        .filter_map(|p| p.ty.wgsl_type().map(|t| (p.key.as_str(), t)))
        .collect();
    if !fields.is_empty() {
        s.push_str("\nstruct BaeParams {\n");
        for (key, ty) in &fields {
            let _ = writeln!(s, "    {key}: {ty},");
        }
        s.push_str("}\n@group(0) @binding(1) var<uniform> params: BaeParams;\n");
    }

    // Sources: rects (host-internal layout) and textures.
    let srcs = sources(effect);
    s.push_str("\nstruct BaeSources {\n");
    for src in &srcs {
        let _ = writeln!(s, "    {}: vec4<i32>,", src.name());
    }
    s.push_str("}\n@group(0) @binding(2) var<uniform> bae_sources: BaeSources;\n");
    let _ = writeln!(
        s,
        "@group(0) @binding(3) var bae_out: texture_storage_2d<{target_format}, write>;"
    );
    for (i, src) in srcs.iter().enumerate() {
        let _ = writeln!(
            s,
            "@group(0) @binding({}) var bae_tex_{}: texture_2d<f32>;",
            BINDING_FIRST_SOURCE as usize + i,
            src.name()
        );
    }

    s.push_str(
        "
fn bae_rect_contains(r: BaeRect, p: vec2<i32>) -> bool {
    return all(p >= r.min) && all(p < r.max);
}

// Render pixel of invocation `id` in the current pass target.
fn bae_target_pixel(id: vec3<u32>) -> vec2<i32> {
    return frame.target_rect.min + vec2<i32>(id.xy);
}

fn bae_in_target(p: vec2<i32>) -> bool {
    return bae_rect_contains(frame.target_rect, p);
}

// Writes the current pass target at render pixel `p` (premultiplied).
fn bae_store(p: vec2<i32>, c: vec4<f32>) {
    textureStore(bae_out, p - frame.target_rect.min, c);
}

// Full-resolution distance → render pixels.
fn bae_scale(x: f32) -> f32 {
    return x * frame.downsample.x;
}

// Render pixels ↔ full-resolution layer pixels.
fn bae_to_layer(p: vec2<f32>) -> vec2<f32> {
    return p / frame.downsample;
}

fn bae_from_layer(p: vec2<f32>) -> vec2<f32> {
    return p * frame.downsample;
}

// 0..1 across the layer bounds.
fn bae_layer_uv(p: vec2<f32>) -> vec2<f32> {
    let r = frame.layer_bounds;
    return (p - vec2<f32>(r.min)) / max(vec2<f32>(r.max - r.min), vec2<f32>(1.0));
}

fn bae_premultiply(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn bae_unpremultiply(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c.rgb / c.a, c.a);
}

fn bae_srgb_to_linear(x: f32) -> f32 {
    let a = abs(x);
    var y: f32;
    if (a <= 0.04045) {
        y = a / 12.92;
    } else {
        y = pow((a + 0.055) / 1.055, 2.4);
    }
    return sign(x) * y;
}

fn bae_linear_to_srgb(x: f32) -> f32 {
    let a = abs(x);
    var y: f32;
    if (a <= 0.0031308) {
        y = a * 12.92;
    } else {
        y = 1.055 * pow(a, 1.0 / 2.4) - 0.055;
    }
    return sign(x) * y;
}

// Working values ↔ linear light (identity when the working space is linear).
// Straight (unpremultiplied) colors.
fn bae_to_linear(c: vec4<f32>) -> vec4<f32> {
    if ((frame.flags & BAE_FRAME_LINEAR) != 0u) {
        return c;
    }
    return vec4<f32>(bae_srgb_to_linear(c.r), bae_srgb_to_linear(c.g), bae_srgb_to_linear(c.b), c.a);
}

fn bae_from_linear(c: vec4<f32>) -> vec4<f32> {
    if ((frame.flags & BAE_FRAME_LINEAR) != 0u) {
        return c;
    }
    return vec4<f32>(bae_linear_to_srgb(c.r), bae_linear_to_srgb(c.g), bae_linear_to_srgb(c.b), c.a);
}

// Rec.709 luma of the working values.
fn bae_luma(c: vec4<f32>) -> f32 {
    return dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Deterministic 0..1 noise (PCG) for a seed and a pixel.
fn bae_hash(seed: u32, p: vec2<i32>) -> f32 {
    var v = seed ^ (bitcast<u32>(p.x) * 747796405u) ^ (bitcast<u32>(p.y) * 2891336453u);
    v = v * 747796405u + 2891336453u;
    let w = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return f32((w >> 22u) ^ w) / 4294967295.0;
}
",
    );

    // Per-source helpers.
    for src in &srcs {
        let n = src.name();
        let _ = write!(
            s,
            "
// Source `{n}`: rect, texel load (transparent outside) and bilinear sample
// (pixel centers at .5).
fn bae_rect_{n}() -> BaeRect {{
    let r = bae_sources.{n};
    return BaeRect(r.xy, r.zw);
}}

fn bae_load_{n}(p: vec2<i32>) -> vec4<f32> {{
    let r = bae_sources.{n};
    if (any(p < r.xy) || any(p >= r.zw)) {{
        return vec4<f32>(0.0);
    }}
    return textureLoad(bae_tex_{n}, p - r.xy, 0);
}}

fn bae_sample_{n}(p: vec2<f32>) -> vec4<f32> {{
    let q = p - vec2<f32>(0.5);
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let a = bae_load_{n}(i);
    let b = bae_load_{n}(i + vec2<i32>(1, 0));
    let c = bae_load_{n}(i + vec2<i32>(0, 1));
    let d = bae_load_{n}(i + vec2<i32>(1, 1));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}}
"
        );
    }

    if effect.shader_style() == ShaderStyle::Pixel {
        s.push_str(
            "
// Pixel style: what the pixel function gets besides the input color.
struct BaePixel {
    pos: vec2<i32>,        // render pixel
    layer_pos: vec2<f32>,  // pixel center in full-resolution layer pixels
    uv: vec2<f32>,         // pixel center, 0..1 across the layer bounds
}
",
        );
    }
    s.push_str("// ---- end of prelude ----\n");
    s
}

/// The generated pixel-style kernel calling the author's pixel function.
pub fn pixel_kernel(entry: &str) -> String {
    format!(
        "
@compute @workgroup_size({wx}, {wy})
fn {PIXEL_KERNEL_ENTRY}(@builtin(global_invocation_id) id: vec3<u32>) {{
    let p = bae_target_pixel(id);
    if (!bae_in_target(p)) {{
        return;
    }}
    let center = vec2<f32>(p) + vec2<f32>(0.5);
    let px = BaePixel(p, bae_to_layer(center), bae_layer_uv(center));
    bae_store(p, {entry}(bae_load_input(p), px));
}}
",
        wx = PIXEL_WORKGROUP[0],
        wy = PIXEL_WORKGROUP[1],
    )
}

/// An author WGSL file (package-relative path and contents).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// Package-relative path (or `plugin:<id>/<path>` for dependency exports).
    pub path: String,
    /// Contents.
    pub text: String,
}

#[derive(Clone, Debug)]
struct Segment {
    file: String,
    start: usize,
    end: usize,
    author: bool,
}

/// An assembled module source with the map back to its files.
#[derive(Clone, Debug)]
pub struct Assembled {
    /// The full WGSL source handed to naga / wgpu.
    pub source: String,
    segments: Vec<Segment>,
}

impl Assembled {
    /// Concatenates prelude, author files and (optionally) the kernel.
    pub fn new(prelude: String, files: &[SourceFile], kernel: Option<String>) -> Self {
        let mut source = String::new();
        let mut segments = Vec::new();
        let mut push = |source: &mut String, file: &str, text: &str, author: bool| {
            let start = source.len();
            source.push_str(text);
            if !text.ends_with('\n') {
                source.push('\n');
            }
            segments.push(Segment {
                file: file.to_owned(),
                start,
                end: source.len(),
                author,
            });
        };
        push(&mut source, PRELUDE_FILE, &prelude, false);
        for f in files {
            push(&mut source, &f.path, &f.text, true);
        }
        if let Some(k) = kernel {
            push(&mut source, KERNEL_FILE, &k, false);
        }
        Self { source, segments }
    }

    fn segment(&self, offset: usize) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|s| offset >= s.start && offset < s.end)
            .or(self.segments.last())
    }

    /// `true` if `offset` lies in an author file.
    fn is_author(&self, offset: usize) -> bool {
        self.segment(offset).is_some_and(|s| s.author)
    }

    /// A diagnostic at byte `offset` of the assembled source.
    pub fn diagnostic_at(&self, offset: usize, message: impl Into<String>) -> Diagnostic {
        match self.segment(offset) {
            Some(seg) => {
                let local = &self.source[seg.start..seg.end];
                let (line, col) = crate::manifest::line_col(local, offset - seg.start);
                Diagnostic::error(seg.file.clone(), message).at(line, col)
            }
            None => Diagnostic::error(PRELUDE_FILE, message),
        }
    }
}

/// A validated module.
#[derive(Debug)]
pub struct CompiledModule {
    /// The assembled source (what wgpu compiles).
    pub assembled: Assembled,
    /// naga IR.
    pub module: naga::Module,
    /// naga analysis (global uses per function / entry point).
    pub info: ModuleInfo,
}

/// Parses and validates an assembled module.
pub fn compile(assembled: Assembled, f16: bool) -> Result<CompiledModule, Vec<Diagnostic>> {
    let module = match naga::front::wgsl::parse_str(&assembled.source) {
        Ok(m) => m,
        Err(e) => {
            let offset = e
                .labels()
                .find_map(|(span, _)| span.to_range())
                .map(|r| r.start);
            let mut message = e.message().to_owned();
            for (span, label) in e.labels() {
                if !label.is_empty() && span.to_range().map(|r| r.start) == offset {
                    message = format!("{message}: {label}");
                    break;
                }
            }
            for note in e.notes() {
                message = format!("{message} ({note})");
            }
            let d = match offset {
                Some(o) => assembled.diagnostic_at(o, message),
                None => Diagnostic::error(PRELUDE_FILE, message),
            };
            return Err(vec![d]);
        }
    };
    let mut caps = Capabilities::default();
    if f16 {
        caps |= Capabilities::SHADER_FLOAT16;
    }
    match Validator::new(ValidationFlags::all(), caps).validate(&module) {
        Ok(info) => Ok(CompiledModule {
            assembled,
            module,
            info,
        }),
        Err(e) => {
            let mut message = e.as_inner().to_string();
            let mut source: Option<&dyn std::error::Error> =
                std::error::Error::source(e.as_inner());
            while let Some(s) = source {
                message = format!("{message}: {s}");
                source = s.source();
            }
            let offset = e
                .spans()
                .find_map(|(span, _)| span.to_range())
                .map(|r| r.start);
            let d = match offset {
                Some(o) => assembled.diagnostic_at(o, message),
                None => Diagnostic::error(PRELUDE_FILE, message),
            };
            Err(vec![d])
        }
    }
}

/// Compiles every module of a declarative effect for `working` and checks the
/// rules of spec §11.2–§11.5 on the IR. Returns one compiled module per pass
/// (pixel style: one, containing the generated kernel).
pub fn compile_effect(
    effect: &EffectDecl,
    files: &[SourceFile],
    working: WorkingFormat,
) -> Result<Vec<CompiledModule>, Vec<Diagnostic>> {
    let f16 = effect.features.iter().any(|f| f == "f16");
    let style = effect.shader_style();
    let formats = pass_target_formats(effect, working);

    // 1. Author code alone (better messages than through the kernel).
    let base = compile(
        Assembled::new(prelude(effect, formats[0]), files, None),
        f16,
    )?;
    let mut errors = check_author_rules(effect, &base);
    if !errors.is_empty() {
        return Err(errors);
    }

    // 2. The modules the host runs.
    let mut modules: Vec<CompiledModule> = Vec::with_capacity(formats.len());
    match style {
        ShaderStyle::Pixel => {
            let kernel = pixel_kernel(&effect.pixel_entry());
            let m = compile(
                Assembled::new(prelude(effect, formats[0]), files, Some(kernel)),
                f16,
            )?;
            modules.push(m);
        }
        ShaderStyle::Compute => {
            for (i, format) in formats.iter().enumerate() {
                let m = compile(Assembled::new(prelude(effect, format), files, None), f16)?;
                errors.extend(check_pass_uses(effect, i, &m));
                modules.push(m);
            }
        }
    }
    if errors.is_empty() {
        Ok(modules)
    } else {
        Err(errors)
    }
}

/// Rules on author code: no bindings, no reserved names, entry points exist.
fn check_author_rules(effect: &EffectDecl, m: &CompiledModule) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let a = &m.assembled;
    let module = &m.module;
    let author_span = |span: naga::Span| span.to_range().filter(|r| a.is_author(r.start));
    let reserved = |name: &str| name.starts_with("bae_") || name.starts_with("Bae");

    for (h, g) in module.global_variables.iter() {
        if let Some(r) = author_span(module.global_variables.get_span(h)) {
            let name = g.name.as_deref().unwrap_or("?");
            if g.binding.is_some() {
                out.push(a.diagnostic_at(
                    r.start,
                    format!(
                        "`{name}` declares @group/@binding; declarative effects get all resources from the prelude ({PRELUDE_DOC})"
                    ),
                ));
            } else if reserved(name) {
                out.push(a.diagnostic_at(
                    r.start,
                    format!("the name `{name}` is reserved (prefix bae_/Bae)"),
                ));
            }
        }
    }
    for (h, f) in module.functions.iter() {
        if let (Some(r), Some(name)) = (author_span(module.functions.get_span(h)), &f.name)
            && reserved(name)
        {
            out.push(a.diagnostic_at(
                r.start,
                format!("the name `{name}` is reserved (prefix bae_/Bae)"),
            ));
        }
    }
    for (h, t) in module.types.iter() {
        if let (Some(r), Some(name)) = (author_span(module.types.get_span(h)), &t.name)
            && reserved(name)
        {
            out.push(a.diagnostic_at(
                r.start,
                format!("the name `{name}` is reserved (prefix bae_/Bae)"),
            ));
        }
    }
    for (h, c) in module.constants.iter() {
        if let (Some(r), Some(name)) = (author_span(module.constants.get_span(h)), &c.name)
            && reserved(name)
        {
            out.push(a.diagnostic_at(
                r.start,
                format!("the name `{name}` is reserved (prefix bae_/Bae)"),
            ));
        }
    }
    let shader = effect.shader.as_deref().unwrap_or("shader");

    match effect.shader_style() {
        ShaderStyle::Pixel => {
            let entry = effect.pixel_entry();
            let found = module
                .functions
                .iter()
                .find(|(_, f)| f.name.as_deref() == Some(entry.as_str()));
            match found {
                None => out.push(Diagnostic::error(
                    shader,
                    format!("pixel function `{entry}` not found; expected `fn {entry}(c: vec4<f32>, px: BaePixel) -> vec4<f32>`"),
                )),
                Some((h, f)) => {
                    let span = module.functions.get_span(h).to_range().map_or(0, |r| r.start);
                    if !pixel_signature_ok(module, f) {
                        out.push(a.diagnostic_at(
                            span,
                            format!("pixel function `{entry}` must have the signature `fn {entry}(c: vec4<f32>, px: BaePixel) -> vec4<f32>`"),
                        ));
                    }
                    let fi = &m.info[h];
                    for (g, var) in module.global_variables.iter() {
                        let is_texture = matches!(
                            module.types[var.ty].inner,
                            naga::TypeInner::Image { .. }
                        );
                        if is_texture && !fi[g].is_empty() {
                            out.push(a.diagnostic_at(
                                span,
                                format!(
                                    "pixel function `{entry}` uses `{}`; pixel functions get the input color as an argument and read no textures (use style = \"compute\")",
                                    var.name.as_deref().unwrap_or("a texture")
                                ),
                            ));
                            break;
                        }
                    }
                }
            }
        }
        ShaderStyle::Compute => {
            for (i, pass) in effect.passes.iter().enumerate() {
                match module.entry_points.iter().find(|e| e.name == pass.entry) {
                    None => out.push(Diagnostic::error(
                        shader,
                        format!(
                            "pass {}: entry point `{}` not found; expected `@compute @workgroup_size(...) fn {}(@builtin(global_invocation_id) id: vec3<u32>)`",
                            i + 1,
                            pass.entry,
                            pass.entry
                        ),
                    )),
                    Some(ep) => {
                        if ep.stage != naga::ShaderStage::Compute {
                            out.push(Diagnostic::error(
                                shader,
                                format!("pass {}: `{}` must be a @compute entry point", i + 1, pass.entry),
                            ));
                        }
                        let n: u64 = ep.workgroup_size.iter().map(|&x| u64::from(x)).product();
                        if n > u64::from(MAX_WORKGROUP_INVOCATIONS) {
                            out.push(Diagnostic::error(
                                shader,
                                format!(
                                    "pass {}: `{}` has {n} invocations per workgroup (max {MAX_WORKGROUP_INVOCATIONS})",
                                    i + 1,
                                    pass.entry
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }
    out
}

fn pixel_signature_ok(module: &naga::Module, f: &naga::Function) -> bool {
    let is_vec4f = |ty: naga::Handle<naga::Type>| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4
                }
            }
        )
    };
    let is_pixel =
        |ty: naga::Handle<naga::Type>| module.types[ty].name.as_deref() == Some("BaePixel");
    f.arguments.len() == 2
        && is_vec4f(f.arguments[0].ty)
        && is_pixel(f.arguments[1].ty)
        && f.result.as_ref().is_some_and(|r| is_vec4f(r.ty))
}

/// A pass may only read the sources it lists and never its own target.
fn check_pass_uses(effect: &EffectDecl, index: usize, m: &CompiledModule) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let pass = &effect.passes[index];
    let Some(ep_index) = m
        .module
        .entry_points
        .iter()
        .position(|e| e.name == pass.entry)
    else {
        return out;
    };
    let info = m.info.get_entry_point(ep_index);
    let shader = effect.shader.as_deref().unwrap_or("shader");
    for (h, var) in m.module.global_variables.iter() {
        let Some(src) = var.name.as_deref().and_then(|n| n.strip_prefix("bae_tex_")) else {
            continue;
        };
        if info[h].contains(GlobalUse::READ) || info[h].contains(GlobalUse::QUERY) {
            if src == pass.writes {
                out.push(Diagnostic::error(
                    shader,
                    format!(
                        "pass {}: `{}` reads its own target `{src}`",
                        index + 1,
                        pass.entry
                    ),
                ));
            } else if !pass.reads.iter().any(|r| r == src) {
                out.push(Diagnostic::error(
                    shader,
                    format!(
                        "pass {}: `{}` reads `{src}` but the manifest does not list it in `reads`",
                        index + 1,
                        pass.entry
                    ),
                ));
            }
        }
    }
    out
}

/// One field of a uniform struct as laid out by naga (WGSL uniform rules).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniformField {
    /// Field name.
    pub name: String,
    /// Byte offset.
    pub offset: u32,
}

/// Layout of a generated uniform struct.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UniformLayout {
    /// Struct size in bytes (the buffer is rounded up to 16).
    pub size: u32,
    /// Fields in declaration order.
    pub fields: Vec<UniformField>,
}

impl UniformLayout {
    /// Offset of a field.
    pub fn offset(&self, name: &str) -> Option<u32> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.offset)
    }

    /// Buffer size: the struct size rounded up to 16 bytes.
    pub fn buffer_size(&self) -> u64 {
        u64::from(self.size.div_ceil(16) * 16)
    }
}

/// The layout of struct `name` (`BaeFrame`, `BaeParams`, `BaeSources`) in a
/// compiled module, `None` if the module has no such struct.
pub fn uniform_layout(module: &naga::Module, name: &str) -> Option<UniformLayout> {
    module.types.iter().find_map(|(_, t)| {
        if t.name.as_deref() != Some(name) {
            return None;
        }
        match &t.inner {
            naga::TypeInner::Struct { members, span } => Some(UniformLayout {
                size: *span,
                fields: members
                    .iter()
                    .map(|m| UniformField {
                        name: m.name.clone().unwrap_or_default(),
                        offset: m.offset,
                    })
                    .collect(),
            }),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;

    fn effect(toml: &str) -> EffectDecl {
        let m = PluginManifest::parse(toml).unwrap();
        let diags = m.validate();
        assert!(!crate::diag::has_errors(&diags), "{diags:#?}");
        m.effects.into_iter().next().unwrap()
    }

    const HEADER: &str = r#"
schema_version = 1
[plugin]
id = "com.example.test"
version = "1.0.0"
api = "0.1"
name = "Test"
"#;

    fn pixel_manifest(extra_params: &str) -> String {
        format!(
            r#"{HEADER}
[[effect]]
id = "tint"
name = "Tint"
implementation = "wgsl"
shader = "tint.wgsl"
[[effect.param]]
id = 1
key = "amount"
type = "float"
name = "Amount"
default = 1.0
range = [0.0, 1.0]
[[effect.param]]
id = 2
key = "color"
type = "color"
name = "Color"
[[effect.param]]
id = 3
key = "center"
type = "point"
name = "Center"
[[effect.param]]
id = 4
key = "mode"
type = "popup"
name = "Mode"
choices = ["A", "B"]
{extra_params}
"#
        )
    }

    fn src(text: &str) -> Vec<SourceFile> {
        vec![SourceFile {
            path: "tint.wgsl".into(),
            text: text.into(),
        }]
    }

    const TINT: &str = "
fn tint(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let k = params.amount * f32(params.mode);
    return mix(c, bae_premultiply(params.color) * c.a, k) + vec4<f32>(params.center, 0.0, 0.0) * 0.0;
}
";

    #[test]
    fn pixel_effect_compiles_with_kernel_and_layouts() {
        let e = effect(&pixel_manifest(""));
        for working in [WorkingFormat::Rgba16Float, WorkingFormat::Rgba32Float] {
            let modules = compile_effect(&e, &src(TINT), working).unwrap();
            assert_eq!(modules.len(), 1);
            let m = &modules[0];
            assert!(
                m.module
                    .entry_points
                    .iter()
                    .any(|ep| ep.name == PIXEL_KERNEL_ENTRY)
            );
            let params = uniform_layout(&m.module, "BaeParams").unwrap();
            // WGSL uniform layout: f32 @0, vec4 @16, vec2 @32, u32 @40, size 48.
            assert_eq!(
                params
                    .fields
                    .iter()
                    .map(|f| (f.name.as_str(), f.offset))
                    .collect::<Vec<_>>(),
                [("amount", 0), ("color", 16), ("center", 32), ("mode", 40)]
            );
            assert_eq!(params.size, 48);
            let frame = uniform_layout(&m.module, "BaeFrame").unwrap();
            assert_eq!(frame.offset("target_rect"), Some(0));
            assert_eq!(frame.offset("layer_bounds"), Some(32));
            assert_eq!(frame.offset("downsample"), Some(48));
            assert_eq!(frame.offset("pass_index"), Some(92));
            assert_eq!(frame.buffer_size(), 96);
            let sources = uniform_layout(&m.module, "BaeSources").unwrap();
            assert_eq!(sources.offset("input"), Some(0));
        }
    }

    #[test]
    fn diagnostics_point_into_author_files() {
        let e = effect(&pixel_manifest(""));
        let bad = "fn tint(c: vec4<f32>, px: BaePixel) -> vec4<f32> {\n    return c * nope;\n}\n";
        let errs = compile_effect(&e, &src(bad), WorkingFormat::default()).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].file, "tint.wgsl");
        assert_eq!(errs[0].line, Some(2));
        assert_eq!(errs[0].column, Some(16));
        assert!(errs[0].message.contains("nope"), "{}", errs[0]);

        let errs = compile_effect(&e, &src("fn other() {}"), WorkingFormat::default()).unwrap_err();
        assert!(
            errs[0].message.contains("pixel function `tint` not found"),
            "{}",
            errs[0]
        );

        let wrong_sig = "fn tint(c: vec4<f32>) -> vec4<f32> { return c; }";
        let errs = compile_effect(&e, &src(wrong_sig), WorkingFormat::default()).unwrap_err();
        assert!(errs[0].message.contains("signature"), "{}", errs[0]);
        assert_eq!(errs[0].line, Some(1));

        let binding = "@group(0) @binding(9) var<uniform> mine: vec4<f32>;\n".to_owned() + TINT;
        let errs = compile_effect(&e, &src(&binding), WorkingFormat::default()).unwrap_err();
        assert!(errs[0].message.contains("@group/@binding"), "{}", errs[0]);

        let reserved = "fn bae_mine() {}\n".to_owned() + TINT;
        let errs = compile_effect(&e, &src(&reserved), WorkingFormat::default()).unwrap_err();
        assert!(errs[0].message.contains("reserved"), "{}", errs[0]);

        let reads = "fn tint(c: vec4<f32>, px: BaePixel) -> vec4<f32> { return bae_load_input(px.pos + vec2<i32>(1, 0)); }";
        let errs = compile_effect(&e, &src(reads), WorkingFormat::default()).unwrap_err();
        assert!(errs[0].message.contains("read no textures"), "{}", errs[0]);
    }

    #[test]
    fn compute_passes_check_reads() {
        let manifest = format!(
            r#"{HEADER}
[[effect]]
id = "blur"
name = "Blur"
implementation = "wgsl"
shader = "blur.wgsl"
[effect.roi]
read = "ceil(params.radius)"
grows = true
[[effect.param]]
id = 1
key = "radius"
type = "float"
name = "Radius"
[[effect.texture]]
name = "tmp"
format = "r32f"
rect = "input"
[[effect.pass]]
entry = "h"
reads = ["input"]
writes = "tmp"
[[effect.pass]]
entry = "v"
reads = ["tmp"]
writes = "output"
"#
        );
        let e = effect(&manifest);
        let good = "
@compute @workgroup_size(8, 8)
fn h(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if (!bae_in_target(p)) { return; }
    bae_store(p, bae_load_input(p) * params.radius);
}
@compute @workgroup_size(8, 8)
fn v(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if (!bae_in_target(p)) { return; }
    bae_store(p, bae_sample_tmp(vec2<f32>(p) + 0.5));
}
";
        let files = vec![SourceFile {
            path: "blur.wgsl".into(),
            text: good.into(),
        }];
        let modules = compile_effect(&e, &files, WorkingFormat::default()).unwrap();
        assert_eq!(modules.len(), 2);
        assert!(
            modules[0]
                .assembled
                .source
                .contains("texture_storage_2d<r32float, write>")
        );
        assert!(
            modules[1]
                .assembled
                .source
                .contains("texture_storage_2d<rgba16float, write>")
        );

        let bad = good.replace("bae_load_input(p) * params.radius", "bae_load_tmp(p)");
        let files = vec![SourceFile {
            path: "blur.wgsl".into(),
            text: bad,
        }];
        let errs = compile_effect(&e, &files, WorkingFormat::default()).unwrap_err();
        assert!(
            errs.iter()
                .any(|d| d.message.contains("reads its own target")),
            "{errs:#?}"
        );

        let big = good.replacen("workgroup_size(8, 8)", "workgroup_size(32, 32)", 1);
        let files = vec![SourceFile {
            path: "blur.wgsl".into(),
            text: big,
        }];
        let errs = compile_effect(&e, &files, WorkingFormat::default()).unwrap_err();
        assert!(errs[0].message.contains("invocations"), "{errs:#?}");
    }
}
