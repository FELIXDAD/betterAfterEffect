//! GPU resources of the compositor: pipelines, bind group layouts, the per-draw
//! record shared with `composite.wgsl`, the mask passes (`mask.wgsl`) and the
//! 8-bit output pass.

use bae_gpu::wgpu;
use bytemuck::{Pod, Zeroable};
use glam::{DAffine2, DMat3, DVec2};
use std::sync::Arc;

use parking_lot::Mutex;
use rustc_hash::FxHashMap;

use crate::three_d::Homography;

use crate::blend::{BlendMode, MaskMode};

/// Coverage accumulation buffers of masks (R: path winding, G: expansion outline).
pub(crate) const MASK_ACCUM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// Mask coverage and combined mask images.
pub(crate) const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

/// A WGSL file without comments, indentation and blank lines, as a `&'static str`
/// built at compile time: the shaders keep their comments in the source but do
/// not ship them inside the executable (about 6.8 KB).
macro_rules! wgsl {
    ($file:literal) => {{
        const SRC: &[u8] = include_bytes!($file);
        const LEN: usize = crate::gpu::minified_len(SRC);
        const OUT: [u8; LEN] = crate::gpu::minify::<LEN>(SRC);
        const TEXT: &str = match std::str::from_utf8(&OUT) {
            Ok(text) => text,
            Err(_) => panic!("WGSL source is not UTF-8"),
        };
        TEXT
    }};
}

/// Line `start..` of `src`: its content without `//` comment and surrounding
/// whitespace (`a..b`) and where the next line starts. WGSL has no string
/// literals, so `//` always starts a comment.
const fn wgsl_line(src: &[u8], start: usize) -> (usize, usize, usize) {
    let mut end = start;
    while end < src.len() && src[end] != b'\n' {
        end += 1;
    }
    let next = if end < src.len() { end + 1 } else { end };
    let mut cut = start;
    while cut < end && !(src[cut] == b'/' && cut + 1 < end && src[cut + 1] == b'/') {
        cut += 1;
    }
    let mut a = start;
    while a < cut && is_blank(src[a]) {
        a += 1;
    }
    let mut b = cut;
    while b > a && is_blank(src[b - 1]) {
        b -= 1;
    }
    (a, b, next)
}

const fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\r'
}

/// Length of [`minify`]'s output.
pub(crate) const fn minified_len(src: &[u8]) -> usize {
    let (mut i, mut n) = (0, 0);
    while i < src.len() {
        let (a, b, next) = wgsl_line(src, i);
        if b > a {
            n += b - a + 1;
        }
        i = next;
    }
    n
}

/// `src` without comments, indentation and blank lines (one line per line).
pub(crate) const fn minify<const N: usize>(src: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let (mut i, mut o) = (0, 0);
    while i < src.len() {
        let (a, b, next) = wgsl_line(src, i);
        if b > a {
            let mut k = a;
            while k < b {
                out[o] = src[k];
                o += 1;
                k += 1;
            }
            out[o] = b'\n';
            o += 1;
        }
        i = next;
    }
    out
}

/// Shader flags (must match `composite.wgsl`).
pub(crate) mod flags {
    pub const TEXTURE: u32 = 1;
    pub const STRAIGHT: u32 = 2;
    pub const MATTE: u32 = 4;
    pub const DST: u32 = 8;
    pub const PRESERVE: u32 = 16;
    pub const CLAMP: u32 = 32;
    pub const OPAQUE: u32 = 64;
    pub const MASK: u32 = 128;
    /// `fwd`/`inv` are full 3×3 homographies (3D layers).
    pub const PROJECTIVE: u32 = 256;
    /// Motion blur: average the samples `extra.y .. extra.y + extra.z`.
    pub const MOTION: u32 = 512;
    /// Lit by the comp's lights: lighting record at vec4 `extra.w`.
    pub const LIT: u32 = 1024;
    /// Motion blur: a lighting record per sample.
    pub const LIT_SAMPLES: u32 = 2048;
    /// 3D plane over 3D models: the matte slot holds the models' camera
    /// depth, `occlusion` the plane's.
    pub const OCCLUDE: u32 = 4096;
}

/// Which fragment entry point a composite pipeline uses: plain draws never pay
/// for the lighting or motion-blur code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Variant {
    /// `fs_main`.
    Plain,
    /// Ordinary textured 2D draws on Metal.
    #[cfg(target_os = "macos")]
    AffineImage,
    /// `fs_lit`: 3D layers lit by the comp's lights.
    Lit,
    /// `fs_motion`: motion-blurred layers (lit or not).
    Motion,
}

/// Most layer-space clip half-planes per draw (3D intersections).
pub(crate) const MAX_CLIPS: usize = crate::three_d::MAX_CLIPS;

/// One draw (layer op), laid out like `struct Draw` in `composite.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct DrawData {
    pub fwd_x: [f32; 4],
    pub fwd_y: [f32; 4],
    pub inv_x: [f32; 4],
    pub inv_y: [f32; 4],
    pub rect: [f32; 4],
    pub quad: [f32; 4],
    pub color: [f32; 4],
    pub src: [f32; 4],
    pub matte: [f32; 4],
    pub target: [f32; 4],
    pub mode: [u32; 4],
    /// Input texel `u` → mask texture uv: `u · xy + zw`.
    pub mask_uv: [f32; 4],
    /// Mask value `a + b · sample` (x, y).
    pub mask_val: [f32; 4],
    /// The mask's region in its atlas page (uv min x, y, max x, y).
    pub mask_clip: [f32; 4],
    /// Forward homography w row (input texel → target texel) and, in `w`, the
    /// near-plane weight below which geometry is clipped.
    pub fwd_w: [f32; 4],
    /// Inverse homography w row.
    pub inv_w: [f32; 4],
    /// Half-planes `a·u + b·v + c ≥ 0` in input texels (`extra.x` of them).
    pub clips: [[f32; 4]; MAX_CLIPS],
    /// Clip count, first motion sample, sample count, lighting record offset.
    pub extra: [u32; 4],
    /// The plane's camera depth `a·x + b·y + c` at target texel (x, y); w: 1
    /// perspective (it is `1 / depth`), 2 orthographic.
    pub occlusion: [f32; 4],
}

/// One motion-blur sample of a draw: target texel → input texel homography,
/// laid out like `struct Sample` in `composite.wgsl`. `inv_w.w` is
/// `1 / near` for 3D samples (pixels closer than the near plane are empty),
/// 0 otherwise.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct SampleData {
    pub inv_x: [f32; 4],
    pub inv_y: [f32; 4],
    pub inv_w: [f32; 4],
}

impl SampleData {
    /// The sample record for `to_target` (input texels → target texels): a 3D
    /// placement clipped at `near`, or (`near = None`) an affine one.
    pub fn new(to_target: &Homography, near: Option<f64>) -> Option<SampleData> {
        let affine = match near {
            Some(_) => None,
            None => Some(to_target.as_affine()?),
        };
        let inv = to_target.inverse()?;
        let r = |i: usize| inv.0.row(i).as_vec3().extend(0.0).to_array();
        let inv_w = match (near, affine) {
            (Some(n), _) => {
                let mut w = r(2);
                w[3] = (1.0 / n) as f32;
                w
            }
            (None, Some(a)) => {
                // Perpendicular edge scales, as in `DrawData::geometry`.
                let det = a.matrix2.determinant().abs();
                let kx = det / a.matrix2.y_axis.length();
                let ky = det / a.matrix2.x_axis.length();
                [kx as f32, ky as f32, 1.0, 0.0]
            }
            (None, None) => return None,
        };
        Some(SampleData {
            inv_x: r(0),
            inv_y: r(1),
            inv_w,
        })
    }
}

impl DrawData {
    /// Fills the geometry from `transform` (input texels → target texels), the
    /// content rectangle in input texels and the target size. `None` when the
    /// transform is degenerate.
    pub fn geometry(transform: &DAffine2, rect: [f64; 4], target: [u32; 2]) -> Option<DrawData> {
        let det = transform.matrix2.determinant();
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let inv = transform.inverse();
        let row = |a: &DAffine2| {
            let (m, t) = (a.matrix2, a.translation);
            (
                [m.x_axis.x as f32, m.y_axis.x as f32, t.x as f32, 0.0],
                [m.x_axis.y as f32, m.y_axis.y as f32, t.y as f32, 0.0],
            )
        };
        let (fwd_x, fwd_y) = row(transform);
        let (inv_x, inv_y) = row(&inv);
        // Perpendicular distance scale: target pixels per input texel across the
        // edges x = const (|det| / |y axis|) and y = const (|det| / |x axis|).
        let kx = det.abs() / transform.matrix2.y_axis.length();
        let ky = det.abs() / transform.matrix2.x_axis.length();
        // Quad margin: the edge ramp reaches half a footprint (max of one target
        // pixel and one input texel) beyond the edge; one full footprint is safe.
        let (mx, my) = ((1.0 / kx).max(1.0), (1.0 / ky).max(1.0));
        Some(DrawData {
            fwd_x,
            fwd_y,
            inv_x,
            inv_y,
            rect: rect.map(|v| v as f32),
            quad: [
                (rect[0] - mx) as f32,
                (rect[1] - my) as f32,
                (rect[2] + mx) as f32,
                (rect[3] + my) as f32,
            ],
            src: [0.0, 0.0, kx as f32, ky as f32],
            target: [target[0] as f32, target[1] as f32, 1.0, 0.0],
            fwd_w: [0.0, 0.0, 1.0, 0.0],
            inv_w: [0.0, 0.0, 1.0, 0.0],
            ..DrawData::default()
        })
    }

    /// Geometry of a projective draw: `h` maps input texels to homogeneous
    /// target texels with `w` = camera depth; geometry closer than `near` is
    /// clipped. `None` when degenerate.
    pub fn projective(
        h: &Homography,
        rect: [f64; 4],
        target: [u32; 2],
        near: f64,
    ) -> Option<DrawData> {
        if !h.is_invertible() {
            return None;
        }
        let inv = h.inverse()?;
        let row = |m: &DMat3, i: usize| m.row(i).as_vec3().extend(0.0).to_array();
        // Quad margin: one target pixel beyond the edges where the layer is
        // smallest on screen (at least one input texel), from the local scales
        // at the corners in front of the camera.
        let corners = [
            DVec2::new(rect[0], rect[1]),
            DVec2::new(rect[2], rect[1]),
            DVec2::new(rect[2], rect[3]),
            DVec2::new(rect[0], rect[3]),
        ];
        let mut k = DVec2::splat(f64::INFINITY);
        for c in corners {
            if h.w(c) >= near {
                k = k.min(h.axis_scales_at(c));
            }
        }
        let limit = (rect[2] - rect[0]).max(rect[3] - rect[1]).max(1.0);
        let margin = |k: f64| {
            if k.is_finite() && k > 0.0 {
                (1.0 / k).clamp(1.0, limit)
            } else {
                1.0
            }
        };
        let (mx, my) = (margin(k.x), margin(k.y));
        let mut fwd_w = row(&h.0, 2);
        fwd_w[3] = near as f32;
        Some(DrawData {
            fwd_x: row(&h.0, 0),
            fwd_y: row(&h.0, 1),
            inv_x: row(&inv.0, 0),
            inv_y: row(&inv.0, 1),
            rect: rect.map(|v| v as f32),
            quad: [
                (rect[0] - mx) as f32,
                (rect[1] - my) as f32,
                (rect[2] + mx) as f32,
                (rect[3] + my) as f32,
            ],
            target: [target[0] as f32, target[1] as f32, 1.0, 0.0],
            mode: [0, flags::PROJECTIVE, 0, 0],
            fwd_w,
            inv_w: row(&inv.0, 2),
            ..DrawData::default()
        })
    }

    /// Geometry of a motion-blurred draw: the quad covers `bounds` (target
    /// texels) directly; the samples map each pixel back into the input.
    pub fn motion(bounds: [f64; 4], rect: [f64; 4], target: [u32; 2]) -> DrawData {
        DrawData {
            fwd_x: [1.0, 0.0, 0.0, 0.0],
            fwd_y: [0.0, 1.0, 0.0, 0.0],
            fwd_w: [0.0, 0.0, 1.0, 0.0],
            inv_x: [1.0, 0.0, 0.0, 0.0],
            inv_y: [0.0, 1.0, 0.0, 0.0],
            inv_w: [0.0, 0.0, 1.0, 0.0],
            rect: rect.map(|v| v as f32),
            quad: bounds.map(|v| v as f32),
            target: [target[0] as f32, target[1] as f32, 1.0, 0.0],
            mode: [0, flags::MOTION, 0, 0],
            ..DrawData::default()
        }
    }

    /// Adds layer-space clip half-planes given in input texels.
    pub fn set_clips(&mut self, clips: &[[f64; 4]]) {
        let n = clips.len().min(MAX_CLIPS);
        for (dst, c) in self.clips.iter_mut().zip(&clips[..n]) {
            *dst = c.map(|v| v as f32);
        }
        self.extra[0] = n as u32;
    }
}

/// How a draw's output is combined with the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BlendKind {
    /// Premultiplied source-over.
    Normal,
    /// Colour `src + dst`, alpha source-over (Add without clamping).
    Add,
    /// Colour `src + dst − src·dst`, alpha source-over.
    Screen,
    /// The shader computes the result from a backdrop copy.
    Replace,
}

impl BlendKind {
    /// The fixed-function kind for `mode`, or `Replace` when the shader must read
    /// the backdrop.
    pub fn for_mode(mode: BlendMode, clamp: bool, preserve: bool, blendable: bool) -> BlendKind {
        if preserve || !blendable {
            return BlendKind::Replace;
        }
        match mode {
            BlendMode::Normal => BlendKind::Normal,
            BlendMode::Add if !clamp => BlendKind::Add,
            BlendMode::Screen => BlendKind::Screen,
            _ => BlendKind::Replace,
        }
    }

    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
        let over = C {
            src_factor: F::One,
            dst_factor: F::OneMinusSrcAlpha,
            operation: O::Add,
        };
        let color = match self {
            BlendKind::Normal => over,
            BlendKind::Add => C {
                src_factor: F::One,
                dst_factor: F::One,
                operation: O::Add,
            },
            BlendKind::Screen => C {
                src_factor: F::OneMinusDst,
                dst_factor: F::One,
                operation: O::Add,
            },
            BlendKind::Replace => return None,
        };
        Some(wgpu::BlendState { color, alpha: over })
    }
}

/// Compiled pipelines and shared objects.
pub(crate) struct Pipelines {
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    pub draws_layout: wgpu::BindGroupLayout,
    pub textures_layout: wgpu::BindGroupLayout,
    /// By target format, blend kind and motion blur (`fs_motion`).
    composite: FxHashMap<(wgpu::TextureFormat, BlendKind, Variant), wgpu::RenderPipeline>,
    pub sampler: wgpu::Sampler,
    pub dummy: wgpu::TextureView,
    pub empty_textures: wgpu::BindGroup,
    pub output_layout: wgpu::BindGroupLayout,
    /// The output conversion into `Rgba8Unorm`.
    pub output: wgpu::RenderPipeline,
    output_module: wgpu::ShaderModule,
    output_pipeline_layout: wgpu::PipelineLayout,
    /// Output conversions into other formats (export), created on first use.
    outputs: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Texture bind groups by the views they bind, with the frame they were
    /// last used in: layer images that persist across frames (remembered
    /// vector sources, cached pre-comps) rebind without a new bind group every
    /// frame. Dropped two frames after their last use ([`Self::end_frame`]).
    bind_groups: FxHashMap<TextureKey, (wgpu::BindGroup, u64)>,
    frame: u64,
    /// The device's shared compiled state (`None` only in the shared base).
    shared: Option<Arc<Shared>>,
}

type CompositeKey = (wgpu::TextureFormat, BlendKind, Variant);

/// Compositor state compiled once per device and shared by every renderer on
/// it (the preview, the render queue, …): the shader modules, layouts and
/// pipelines. The app's warm-up compiles the common pipelines in advance
/// ([`CompositorWarmUp`]); a pipeline the UI thread needs before that is
/// compiled in the background and the frame is incomplete meanwhile.
pub(crate) struct Shared {
    device: wgpu::Device,
    outputs: Mutex<FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>>,
    base: Pipelines,
    /// `None`: being compiled in the background.
    composite: Mutex<FxHashMap<CompositeKey, Option<wgpu::RenderPipeline>>>,
    pub masks: Arc<Mutex<Option<MaskPipelines>>>,
    pub mix: Arc<Mutex<Option<crate::exec::adjust::MixPipelines>>>,
    pub dof: Arc<Mutex<Option<crate::exec::dof::DofPipelines>>>,
    pub mesh: Arc<Mutex<Option<crate::exec::mesh::MeshPipelines>>>,
    pub shadow_map: Arc<Mutex<Option<crate::exec::shadowmap::ShadowMapPipeline>>>,
}

static SHARED: Mutex<Vec<(std::sync::Weak<bae_gpu::GpuContext>, Arc<Shared>)>> =
    Mutex::new(Vec::new());

impl Shared {
    /// The shared state of `gpu` (built on first use).
    pub fn get(gpu: &Arc<bae_gpu::GpuContext>) -> Arc<Shared> {
        let mut all = SHARED.lock();
        all.retain(|(g, _)| g.strong_count() > 0);
        if let Some((_, s)) = all
            .iter()
            .find(|(g, _)| std::ptr::eq(g.as_ptr(), Arc::as_ptr(gpu)))
        {
            return s.clone();
        }
        let shared = Arc::new(Shared {
            device: gpu.device().clone(),
            outputs: Default::default(),
            base: Pipelines::build(gpu.device()),
            composite: Mutex::new(FxHashMap::default()),
            masks: Arc::new(Mutex::new(None)),
            mix: Arc::new(Mutex::new(None)),
            dof: Arc::new(Mutex::new(None)),
            mesh: Arc::new(Mutex::new(None)),
            shadow_map: Arc::new(Mutex::new(None)),
        });
        all.push((Arc::downgrade(gpu), shared.clone()));
        shared
    }

    /// A renderer's view of the shared state.
    fn fork(self: &Arc<Self>) -> Pipelines {
        let b = &self.base;
        Pipelines {
            module: b.module.clone(),
            layout: b.layout.clone(),
            draws_layout: b.draws_layout.clone(),
            textures_layout: b.textures_layout.clone(),
            composite: FxHashMap::default(),
            sampler: b.sampler.clone(),
            dummy: b.dummy.clone(),
            empty_textures: b.empty_textures.clone(),
            output_layout: b.output_layout.clone(),
            output: b.output.clone(),
            output_module: b.output_module.clone(),
            output_pipeline_layout: b.output_pipeline_layout.clone(),
            outputs: FxHashMap::default(),
            bind_groups: FxHashMap::default(),
            frame: 0,
            shared: Some(self.clone()),
        }
    }

    /// The composite pipeline for `key`; on an interactive thread `None`
    /// while it compiles in the background (a redraw is requested when done).
    fn composite(self: &Arc<Self>, key: CompositeKey) -> Option<wgpu::RenderPipeline> {
        let interactive = crate::is_interactive_thread();
        match self.composite.lock().get(&key) {
            Some(Some(p)) => return Some(p.clone()),
            Some(None) if interactive => return None,
            _ => {}
        }
        if interactive {
            self.composite.lock().insert(key, None);
            let shared = self.clone();
            let spawned = std::thread::Builder::new()
                .name("bae-render-compile".into())
                .spawn(move || {
                    let p = shared.create(key);
                    shared.composite.lock().insert(key, Some(p));
                    bae_gpu::warm_up::request_redraw();
                });
            if spawned.is_ok() {
                return None;
            }
        }
        let p = self.create(key);
        self.composite.lock().insert(key, Some(p.clone()));
        Some(p)
    }

    fn create(&self, key: CompositeKey) -> wgpu::RenderPipeline {
        composite_pipeline(&self.device, &self.base.module, &self.base.layout, key)
    }

    /// Compiles the composite pipeline `key` unless it exists.
    fn warm_one(&self, key: CompositeKey) {
        if matches!(self.composite.lock().get(&key), Some(Some(_))) {
            return;
        }
        let p = self.create(key);
        self.composite.lock().insert(key, Some(p));
    }

    /// Compiles the composite pipelines of `formats` (with whether the format
    /// blends in fixed function), the mask passes and the adjustment mixes.
    pub fn warm(&self, formats: &[(wgpu::TextureFormat, bool)]) {
        for &(format, blendable) in formats {
            let kinds: &[BlendKind] = if blendable {
                &[
                    BlendKind::Normal,
                    BlendKind::Add,
                    BlendKind::Screen,
                    BlendKind::Replace,
                ]
            } else {
                &[BlendKind::Replace]
            };
            #[cfg(target_os = "macos")]
            if blendable {
                self.warm_one((format, BlendKind::Normal, Variant::AffineImage));
            }
            for &kind in kinds {
                for variant in [Variant::Plain, Variant::Lit, Variant::Motion] {
                    self.warm_one((format, kind, variant));
                }
            }
        }
        // Built outside the locks, kept unless a renderer made one meanwhile.
        if self.masks.lock().is_none() {
            let mut masks = MaskPipelines::new(&self.device);
            masks.warm(&self.device);
            self.masks.lock().get_or_insert(masks);
        }
        if self.mix.lock().is_none() {
            let mut mix = crate::exec::adjust::MixPipelines::new(&self.device);
            for &(format, _) in formats {
                mix.warm(&self.device, format);
            }
            self.mix.lock().get_or_insert(mix);
        }
        if self.dof.lock().is_none() {
            let mut dof = crate::exec::dof::DofPipelines::new(&self.device);
            for &(format, _) in formats {
                dof.warm(&self.device, format);
            }
            self.dof.lock().get_or_insert(dof);
        }
        if self.mesh.lock().is_none() {
            let mut mesh = crate::exec::mesh::MeshPipelines::new(&self.device);
            for &(format, _) in formats {
                mesh.warm(&self.device, format);
            }
            self.mesh.lock().get_or_insert(mesh);
        }
        if self.shadow_map.lock().is_none() {
            let pipeline = crate::exec::shadowmap::ShadowMapPipeline::new(&self.device);
            self.shadow_map.lock().get_or_insert(pipeline);
        }
    }
}

/// Compiles the compositor's pipelines during the app's warm-up: first only
/// what plain text and shape layers need ([`CompositorWarmUp::Basic`], right
/// after the vector renderer, so a layer created early shows at once), the
/// other formats, blend kinds, variants, mask passes and mixes later
/// ([`CompositorWarmUp::Full`], before the effects).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorWarmUp {
    /// RGBA16F, normal blending, no lighting or motion blur.
    Basic,
    /// Everything else.
    Full,
}

impl bae_gpu::WarmUp for CompositorWarmUp {
    fn name(&self) -> &str {
        match self {
            CompositorWarmUp::Basic => "compositor (text and shapes)",
            CompositorWarmUp::Full => "compositor",
        }
    }

    fn priority(&self) -> u8 {
        match self {
            CompositorWarmUp::Basic => 2,
            CompositorWarmUp::Full => 10,
        }
    }

    fn run(&self, gpu: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        let shared = Shared::get(gpu);
        if *self == CompositorWarmUp::Basic {
            shared.warm_one((
                wgpu::TextureFormat::Rgba16Float,
                BlendKind::Normal,
                Variant::Plain,
            ));
            #[cfg(target_os = "macos")]
            shared.warm_one((
                wgpu::TextureFormat::Rgba16Float,
                BlendKind::Normal,
                Variant::AffineImage,
            ));
            return Ok(());
        }
        let float32 = wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::FLOAT32_BLENDABLE;
        let mut formats = vec![(wgpu::TextureFormat::Rgba16Float, true)];
        if gpu.features().contains(float32) {
            formats.push((wgpu::TextureFormat::Rgba32Float, true));
        }
        #[cfg(target_os = "macos")]
        {
            let mut pipelines = shared.fork();
            for format in [
                wgpu::TextureFormat::Bgra8Unorm,
                wgpu::TextureFormat::Rgba16Float,
            ] {
                pipelines.output_for(gpu.device(), format);
            }
        }
        shared.warm(&formats);
        Ok(())
    }
}

/// The views of a texture bind group (src, matte, dst, mask, shadow).
type TextureKey = [Option<wgpu::TextureView>; 5];

impl Pipelines {
    /// The renderer's pipelines: a view of the device's shared state.
    pub fn new(gpu: &Arc<bae_gpu::GpuContext>) -> Self {
        Shared::get(gpu).fork()
    }

    /// The shared base: shader modules, layouts and the RGBA8 output.
    fn build(device: &wgpu::Device) -> Self {
        let composite = [wgsl!("composite.wgsl"), wgsl!("shadow.wgsl")].concat();
        #[cfg(target_os = "macos")]
        let composite = [composite.as_str(), wgsl!("metal_image.wgsl")].concat();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.composite"),
            source: wgpu::ShaderSource::Wgsl(composite.into()),
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        // Draw records, motion-blur samples and lighting records.
        let draws_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.draws"),
            entries: &[storage(0), storage(1), storage(2)],
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let textures_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.textures"),
            entries: &[
                tex(0),
                tex(1),
                tex(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex(4),
                tex(5),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.composite"),
            bind_group_layouts: &[Some(&draws_layout), Some(&textures_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.dummy"),
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
        let empty_textures = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.empty"),
            layout: &textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
            ],
        });

        let output_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.output"),
            source: wgpu::ShaderSource::Wgsl(wgsl!("output.wgsl").into()),
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.output"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let output_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("bae-render.output"),
                bind_group_layouts: &[Some(&output_layout)],
                immediate_size: 0,
            });
        let output = output_pipeline(
            device,
            &output_module,
            &output_pipeline_layout,
            wgpu::TextureFormat::Rgba8Unorm,
        );

        Self {
            module,
            layout,
            draws_layout,
            textures_layout,
            composite: FxHashMap::default(),
            sampler,
            dummy,
            empty_textures,
            output_layout,
            output,
            output_module,
            output_pipeline_layout,
            outputs: Default::default(),
            bind_groups: FxHashMap::default(),
            frame: 0,
            shared: None,
        }
    }

    /// Ends a frame: forgets bind groups unused in the last two frames (they
    /// hold their textures).
    pub fn end_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.bind_groups.retain(|_, (_, used)| frame - *used <= 2);
    }

    /// The shared mask pipelines (created on first use).
    pub fn masks(&self) -> Arc<Mutex<Option<MaskPipelines>>> {
        self.shared
            .as_ref()
            .map(|s| s.masks.clone())
            .unwrap_or_default()
    }

    /// The shared adjustment-layer mix pipelines (created on first use).
    pub fn mix(&self) -> Arc<Mutex<Option<crate::exec::adjust::MixPipelines>>> {
        self.shared
            .as_ref()
            .map(|s| s.mix.clone())
            .unwrap_or_default()
    }

    /// The shared 3D model pipelines and uploaded models (created on first use).
    pub fn mesh(&self) -> Arc<Mutex<Option<crate::exec::mesh::MeshPipelines>>> {
        self.shared
            .as_ref()
            .map(|s| s.mesh.clone())
            .unwrap_or_default()
    }

    /// The shared depth-of-field pipelines (created on first use).
    pub fn dof(&self) -> Arc<Mutex<Option<crate::exec::dof::DofPipelines>>> {
        self.shared
            .as_ref()
            .map(|s| s.dof.clone())
            .unwrap_or_default()
    }

    /// The shared shadow-map pipeline (created on first use).
    pub fn shadow_map(&self) -> Arc<Mutex<Option<crate::exec::shadowmap::ShadowMapPipeline>>> {
        self.shared
            .as_ref()
            .map(|s| s.shadow_map.clone())
            .unwrap_or_default()
    }

    /// The output conversion pipeline rendering into `format`.
    pub fn output_for(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        if format == wgpu::TextureFormat::Rgba8Unorm {
            return &self.output;
        }
        let build = || {
            output_pipeline(
                device,
                &self.output_module,
                &self.output_pipeline_layout,
                format,
            )
        };
        self.outputs
            .entry(format)
            .or_insert_with(|| match &self.shared {
                Some(shared) => shared
                    .outputs
                    .lock()
                    .entry(format)
                    .or_insert_with(build)
                    .clone(),
                None => build(),
            })
    }

    /// The composite pipeline for a target format, blend kind and fragment
    /// variant, from the device's shared pipelines. `None` on an interactive
    /// thread while it is still being compiled (the frame is incomplete).
    pub fn composite(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        kind: BlendKind,
        variant: Variant,
    ) -> Option<&wgpu::RenderPipeline> {
        let key = (format, kind, variant);
        if !self.composite.contains_key(&key) {
            let pipeline = match &self.shared {
                Some(shared) => shared.composite(key)?,
                None => composite_pipeline(device, &self.module, &self.layout, key),
            };
            self.composite.insert(key, pipeline);
        }
        self.composite.get(&key)
    }

    /// A texture bind group (unused slots get the dummy texture; `shadow` is
    /// the shadow atlas of lit layers).
    pub fn textures(
        &mut self,
        device: &wgpu::Device,
        src: Option<&wgpu::TextureView>,
        matte: Option<&wgpu::TextureView>,
        dst: Option<&wgpu::TextureView>,
        mask: Option<&wgpu::TextureView>,
        shadow: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        let key: TextureKey = [src, matte, dst, mask, shadow].map(|v| v.cloned());
        let frame = self.frame;
        if let Some((group, used)) = self.bind_groups.get_mut(&key) {
            *used = frame;
            return group.clone();
        }
        let group = self.create_textures(device, src, matte, dst, mask, shadow);
        self.bind_groups.insert(key, (group.clone(), frame));
        group
    }

    fn create_textures(
        &self,
        device: &wgpu::Device,
        src: Option<&wgpu::TextureView>,
        matte: Option<&wgpu::TextureView>,
        dst: Option<&wgpu::TextureView>,
        mask: Option<&wgpu::TextureView>,
        shadow: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.textures"),
            layout: &self.textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(matte.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(dst.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(mask.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(shadow.unwrap_or(&self.dummy)),
                },
            ],
        })
    }
}

/// How a mask pass writes its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MaskBlend {
    /// Overwrite (resolve and blur passes).
    Replace,
    /// Merge into the accumulator with a mask mode (AE-CMP-038).
    Mode(MaskMode),
}

impl MaskBlend {
    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
        let c = |src_factor, dst_factor, operation| C {
            src_factor,
            dst_factor,
            operation,
        };
        let component = match self {
            MaskBlend::Replace | MaskBlend::Mode(MaskMode::None) => return None,
            // acc + m − acc·m
            MaskBlend::Mode(MaskMode::Add) => c(F::One, F::OneMinusSrc, O::Add),
            // acc · (1 − m)
            MaskBlend::Mode(MaskMode::Subtract) => c(F::Zero, F::OneMinusSrc, O::Add),
            // acc · m
            MaskBlend::Mode(MaskMode::Intersect) => c(F::Zero, F::Src, O::Add),
            MaskBlend::Mode(MaskMode::Lighten) => c(F::One, F::One, O::Max),
            MaskBlend::Mode(MaskMode::Darken) => c(F::One, F::One, O::Min),
            // m·(1 − acc) + acc·(1 − m) = acc + m − 2·acc·m
            MaskBlend::Mode(MaskMode::Difference) => c(F::OneMinusDst, F::OneMinusSrc, O::Add),
        };
        Some(wgpu::BlendState {
            color: component,
            alpha: component,
        })
    }
}

/// One mask edge, laid out like `struct Edge` in `mask.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct EdgeData {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub xmax: f32,
    pub chan: u32,
    pub size: [f32; 2],
    /// Winding contribution (1; 1/N for variable-width feather contours).
    pub weight: f32,
    pub _pad: f32,
}

/// One full-screen mask pass, laid out like `struct Pass` in `mask.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct MaskPassData {
    pub map: [f32; 4],
    pub src: [f32; 4],
    pub blur: [f32; 4],
    pub clip: [f32; 4],
    pub value: [f32; 4],
    pub mode: [u32; 4],
    pub src_rect: [f32; 4],
}

/// Pipelines of `mask.wgsl`.
pub(crate) struct MaskPipelines {
    module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pub layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    pub edges: wgpu::RenderPipeline,
    passes: FxHashMap<MaskBlend, wgpu::RenderPipeline>,
}

impl MaskPipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.mask"),
            source: wgpu::ShaderSource::Wgsl(wgsl!("mask.wgsl").into()),
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.mask"),
            entries: &[
                storage(0),
                storage(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.mask"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.mask"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let edges = mask_pipeline(
            device,
            &module,
            &pipeline_layout,
            ("vs_edge", "fs_edge"),
            MASK_ACCUM_FORMAT,
            Some(wgpu::BlendState {
                color: additive,
                alpha: additive,
            }),
        );
        Self {
            module,
            pipeline_layout,
            layout,
            sampler,
            edges,
            passes: FxHashMap::default(),
        }
    }

    /// Compiles every pass (the warm-up).
    fn warm(&mut self, device: &wgpu::Device) {
        use crate::blend::MaskMode;
        for blend in [
            MaskBlend::Replace,
            MaskBlend::Mode(MaskMode::Add),
            MaskBlend::Mode(MaskMode::Subtract),
            MaskBlend::Mode(MaskMode::Intersect),
            MaskBlend::Mode(MaskMode::Lighten),
            MaskBlend::Mode(MaskMode::Darken),
            MaskBlend::Mode(MaskMode::Difference),
        ] {
            self.pass(device, blend);
        }
    }

    /// The full-screen pass writing `MASK_FORMAT` with `blend`.
    pub fn pass(&mut self, device: &wgpu::Device, blend: MaskBlend) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.passes.entry(blend).or_insert_with(|| {
            mask_pipeline(
                device,
                module,
                layout,
                ("vs_full", "fs_pass"),
                MASK_FORMAT,
                blend.state(),
            )
        })
    }
}

fn mask_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    (vs, fs): (&str, &str),
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bae-render.mask"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// A full-screen pass converting a working buffer into `format` (`output.wgsl`).
fn output_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bae-render.output"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_full"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_out"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// A composite pipeline (`composite.wgsl`) for a target format, blend kind and
/// fragment variant.
fn composite_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    (format, kind, variant): CompositeKey,
) -> wgpu::RenderPipeline {
    let (vertex, fragment) = match variant {
        Variant::Plain => ("vs_main", "fs_main"),
        Variant::Lit => ("vs_main", "fs_lit"),
        Variant::Motion => ("vs_main", "fs_motion"),
        #[cfg(target_os = "macos")]
        Variant::AffineImage => ("vs_image", "fs_image"),
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bae-render.composite"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: kind.state(),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;

    #[test]
    fn draw_record_matches_wgsl_layout() {
        // 22 vec4s: the WGSL struct stride.
        assert_eq!(std::mem::size_of::<DrawData>(), 352);
        assert_eq!(std::mem::size_of::<SampleData>(), 48);
        assert_eq!(std::mem::size_of::<EdgeData>(), 40);
        assert_eq!(std::mem::size_of::<MaskPassData>(), 112);
    }

    #[test]
    fn shaders_ship_without_comments() {
        const SRC: &[u8] = b"// header\n  let a = 1; // note\n\n\tfn f() {}\n";
        const LEN: usize = minified_len(SRC);
        assert_eq!(&minify::<LEN>(SRC), b"let a = 1;\nfn f() {}\n");
        let composite: &str = wgsl!("composite.wgsl");
        assert!(!composite.contains("//") && composite.contains("fn fs_main"));
        assert!(composite.len() < include_str!("composite.wgsl").len() * 3 / 4);
    }

    #[test]
    fn geometry_margins_follow_scale() {
        let t = DAffine2::from_scale(DVec2::new(2.0, 0.5));
        let d = DrawData::geometry(&t, [0.0, 0.0, 10.0, 10.0], [100, 100]).unwrap();
        // Margin = max(1 target pixel, 1 input texel): 1 texel along x (magnified),
        // 2 texels (= 1 target pixel) along y.
        assert_eq!(d.quad, [-1.0, -2.0, 11.0, 12.0]);
        assert_eq!(&d.src[2..], &[2.0, 0.5]);
        assert!(DrawData::geometry(&DAffine2::from_scale(DVec2::ZERO), [0.0; 4], [1, 1]).is_none());
    }
}
