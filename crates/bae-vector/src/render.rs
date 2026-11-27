//! GPU drawing with Vello (feature `render`).
//!
//! * [`encode`] appends a [`VectorScene`] to a `vello::Scene`.
//! * [`VelloRenderer`] owns one `vello::Renderer` per [`GpuContext`] (pipelines
//!   are compiled once and shared by shapes, text and anyone else drawing vectors)
//!   and renders scenes into textures on the shared device.
//! * [`ShapeSource`] / [`ShapeProvider`] plug shape layers into bae-render's
//!   [`LayerSource`] / [`SourceProvider`] interface ([`crate::SHAPE_PROVIDER`]).
//!
//! Vello writes `Rgba8Unorm` with **straight** alpha; images are returned as
//! [`ImageAlpha::Straight`] and the compositor premultiplies when sampling. Colour
//! values pass through unchanged (they are already working-space values).
//!
//! On software adapters (WARP on GPU-less Windows machines, VMs, remote desktop)
//! Vello's compute pipelines crash the process, so [`VelloRenderer`] never creates
//! them there: it rasterizes with `vello_cpu` instead and uploads the result
//! (premultiplied `Rgba8Unorm`). See [`VectorBackend`].

use std::sync::{Arc, Weak};

use bae_anim::EvaluatedLayer;
use bae_gpu::{GpuContext, wgpu};
use bae_model::Time;
use bae_render::{
    CustomContent, EncodeContext, GpuImage, IRect, ImageAlpha, LayerSource, RenderError,
    SourceProvider, SourceRequest,
};
use kurbo::{Affine, Rect};
use parking_lot::{Condvar, Mutex};
use rustc_hash::FxHashMap;
use vello::peniko::{self, Color, ColorStop, Extend, InterpolationAlphaSpace};

use crate::paint::{BlendMode, Brush, FillRule, Gradient, GradientKind};
use crate::scene::{DrawItem, VectorScene};

/// Texture format Vello renders to.
pub const VELLO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Appends `vs` to `scene` with `transform` (layer space → target pixels).
/// `viewport` is the target rectangle in target pixels; isolated layers are
/// clipped to it.
pub fn encode(scene: &mut vello::Scene, vs: &VectorScene, transform: Affine, viewport: Rect) {
    for item in &vs.items {
        encode_item(scene, item, transform, viewport);
    }
}

fn encode_item(scene: &mut vello::Scene, item: &DrawItem, transform: Affine, viewport: Rect) {
    match item {
        DrawItem::Fill(f) => {
            let style = match f.rule {
                FillRule::NonZero => peniko::Fill::NonZero,
                FillRule::EvenOdd => peniko::Fill::EvenOdd,
            };
            let brush = to_brush(&f.brush, f.opacity);
            scene.fill(style, transform * f.transform, &brush, None, &f.path);
        }
        DrawItem::Stroke(s) => {
            let brush = to_brush(&s.brush, s.opacity);
            scene.stroke(
                &s.style.to_kurbo(),
                transform * s.transform,
                &brush,
                None,
                &s.path,
            );
        }
        DrawItem::Layer(l) => {
            scene.push_layer(
                peniko::Fill::NonZero,
                to_blend(l.blend),
                l.opacity,
                Affine::IDENTITY,
                &viewport,
            );
            for i in &l.items {
                encode_item(scene, i, transform, viewport);
            }
            scene.pop_layer();
        }
    }
}

/// A paint brush as a peniko brush with `opacity` folded in.
pub fn to_brush(brush: &Brush, opacity: f32) -> peniko::Brush {
    match brush {
        Brush::Solid(c) => peniko::Brush::Solid(Color::new([c[0], c[1], c[2], c[3] * opacity])),
        Brush::Gradient(g) => peniko::Brush::Gradient(to_gradient(g, opacity)),
    }
}

fn to_gradient(g: &Gradient, opacity: f32) -> peniko::Gradient {
    let mut out = match g.kind {
        GradientKind::Linear => peniko::Gradient::new_linear(g.start, g.end),
        GradientKind::Radial {
            highlight_length,
            highlight_angle,
        } => {
            // AE-SHP-028: focal point on the start → end direction, rotated by
            // the highlight angle, at `h · r` from the start (|h| < 1).
            let d = g.end - g.start;
            let r = d.hypot();
            let h = (highlight_length / 100.0).clamp(-0.99, 0.99);
            let a = d.y.atan2(d.x) + highlight_angle.to_radians();
            let focal = g.start + kurbo::Vec2::new(a.cos(), a.sin()) * (h * r);
            peniko::Gradient::new_two_point_radial(focal, 0.0, g.start, r as f32)
        }
    };
    out.extend = Extend::Pad;
    out.interpolation_alpha_space = InterpolationAlphaSpace::Unpremultiplied;
    let stops: Vec<ColorStop> = g
        .stops
        .iter()
        .map(|s| ColorStop {
            offset: s.offset,
            color: Color::new([s.color[0], s.color[1], s.color[2], s.color[3] * opacity]).into(),
        })
        .collect();
    out.with_stops(stops.as_slice())
}

fn to_blend(b: BlendMode) -> peniko::BlendMode {
    use peniko::Mix;
    let mix = match b {
        BlendMode::Normal => Mix::Normal,
        BlendMode::Multiply => Mix::Multiply,
        BlendMode::Screen => Mix::Screen,
        BlendMode::Overlay => Mix::Overlay,
        BlendMode::Darken => Mix::Darken,
        BlendMode::Lighten => Mix::Lighten,
        BlendMode::ColorDodge => Mix::ColorDodge,
        BlendMode::ColorBurn => Mix::ColorBurn,
        BlendMode::HardLight => Mix::HardLight,
        BlendMode::SoftLight => Mix::SoftLight,
        BlendMode::Difference => Mix::Difference,
        BlendMode::Exclusion => Mix::Exclusion,
        BlendMode::Hue => Mix::Hue,
        BlendMode::Saturation => Mix::Saturation,
        BlendMode::Color => Mix::Color,
        BlendMode::Luminosity => Mix::Luminosity,
    };
    mix.into()
}

/// Which rasterizer a [`VelloRenderer`] uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VectorBackend {
    /// Vello's GPU compute pipelines (hardware adapters).
    Gpu,
    /// `vello_cpu` on the CPU, uploaded as a texture. Used on software adapters
    /// (WARP on GPU-less Windows: Vello's compute pipelines crash the process there)
    /// and when forced with `BAE_VECTOR_RENDERER=cpu`.
    Cpu,
}

/// Environment variable that forces the vector backend: `cpu` or `gpu`
/// (`gpu` also on software adapters, to reproduce driver problems).
pub const BACKEND_ENV: &str = "BAE_VECTOR_RENDERER";

impl VectorBackend {
    /// The backend for `gpu`: CPU on software adapters, GPU otherwise, unless
    /// [`BACKEND_ENV`] says otherwise.
    pub fn for_adapter(gpu: &GpuContext) -> Self {
        match std::env::var(BACKEND_ENV).ok().as_deref() {
            Some("cpu") => VectorBackend::Cpu,
            Some("gpu") => VectorBackend::Gpu,
            _ if gpu.adapter_info().device_type == wgpu::DeviceType::Cpu => VectorBackend::Cpu,
            _ => VectorBackend::Gpu,
        }
    }
}

/// Something the vector renderer can draw in layer space, on either backend.
pub trait VectorDraw {
    /// Appends the content to a Vello scene; `transform` maps layer space to
    /// target texels, `viewport` is the target rectangle in texels.
    fn encode(&self, scene: &mut vello::Scene, transform: Affine, viewport: Rect);

    /// Draws the content with `vello_cpu`; `transform` as for [`VectorDraw::encode`].
    fn draw_cpu(&self, ctx: &mut vello_cpu::RenderContext, transform: Affine);
}

impl VectorDraw for VectorScene {
    fn encode(&self, scene: &mut vello::Scene, transform: Affine, viewport: Rect) {
        encode(scene, self, transform, viewport);
    }

    fn draw_cpu(&self, ctx: &mut vello_cpu::RenderContext, transform: Affine) {
        draw_cpu(ctx, self, transform);
    }
}

/// Draws `vs` with `vello_cpu` (the software fallback of [`encode`]).
pub fn draw_cpu(ctx: &mut vello_cpu::RenderContext, vs: &VectorScene, transform: Affine) {
    for item in &vs.items {
        draw_item_cpu(ctx, item, transform);
    }
}

fn draw_item_cpu(ctx: &mut vello_cpu::RenderContext, item: &DrawItem, transform: Affine) {
    match item {
        DrawItem::Fill(f) => {
            ctx.set_transform(transform * f.transform);
            ctx.set_paint(cpu_paint(to_brush(&f.brush, f.opacity)));
            ctx.set_fill_rule(match f.rule {
                FillRule::NonZero => peniko::Fill::NonZero,
                FillRule::EvenOdd => peniko::Fill::EvenOdd,
            });
            ctx.fill_path(&f.path);
        }
        DrawItem::Stroke(s) => {
            ctx.set_transform(transform * s.transform);
            ctx.set_paint(cpu_paint(to_brush(&s.brush, s.opacity)));
            ctx.set_stroke(s.style.to_kurbo());
            ctx.stroke_path(&s.path);
        }
        DrawItem::Layer(l) => {
            ctx.set_transform(Affine::IDENTITY);
            ctx.push_layer(None, Some(to_blend(l.blend)), Some(l.opacity), None, None);
            for i in &l.items {
                draw_item_cpu(ctx, i, transform);
            }
            ctx.pop_layer();
        }
    }
}

/// A peniko brush as a `vello_cpu` paint (solid colours and gradients).
pub fn cpu_paint(brush: peniko::Brush) -> vello_cpu::PaintType {
    match brush {
        peniko::Brush::Solid(c) => vello_cpu::PaintType::Solid(c),
        peniko::Brush::Gradient(g) => vello_cpu::PaintType::Gradient(g),
        peniko::Brush::Image(_) => vello_cpu::PaintType::Solid(Color::TRANSPARENT),
    }
}

/// The shared vector rasterizer for one GPU context: Vello on the GPU, or
/// `vello_cpu` on software adapters ([`VectorBackend`]).
pub struct VelloRenderer {
    backend: VectorBackend,
    /// `None` on the CPU backend (Vello's pipelines are never created there).
    renderer: Option<Mutex<vello::Renderer>>,
}

impl std::fmt::Debug for VelloRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VelloRenderer")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

/// Renderers by GPU context, kept for the context's lifetime: Vello compiles
/// its pipelines when a renderer is created, so letting it go between draws
/// recompiled every shader for every shape layer of every frame.
type SharedRenderers = Mutex<Vec<(Weak<GpuContext>, Slot)>>;
static SHARED: SharedRenderers = Mutex::new(Vec::new());
/// Signalled when a shared renderer finished building (or failed).
static BUILT: Condvar = Condvar::new();

/// A shared renderer, or one being built (by the warm-up, usually).
enum Slot {
    Building,
    Ready(Arc<VelloRenderer>),
}

/// Threads Vello compiles its shaders on when the user is not waiting (the
/// warm-up): few, so the UI keeps the CPU.
const BACKGROUND_COMPILE_THREADS: usize = 2;

fn vello_error(e: vello::Error) -> RenderError {
    RenderError::Other(format!("vello: {e}"))
}

impl VelloRenderer {
    /// A new renderer on `gpu` with the backend chosen by
    /// [`VectorBackend::for_adapter`]. Prefer [`VelloRenderer::shared`].
    pub fn new(gpu: &GpuContext) -> Result<Self, RenderError> {
        Self::with_backend(gpu, VectorBackend::for_adapter(gpu))
    }

    /// A new renderer with an explicit backend. The GPU backend compiles Vello's
    /// pipelines (tens to hundreds of milliseconds).
    pub fn with_backend(gpu: &GpuContext, backend: VectorBackend) -> Result<Self, RenderError> {
        Self::build(gpu, backend, None)
    }

    /// A renderer compiling Vello's shaders on `threads` threads (all cores when
    /// `None`).
    fn build(
        gpu: &GpuContext,
        backend: VectorBackend,
        threads: Option<std::num::NonZeroUsize>,
    ) -> Result<Self, RenderError> {
        let renderer = match backend {
            VectorBackend::Cpu => None,
            VectorBackend::Gpu => Some(Mutex::new(
                vello::Renderer::new(
                    gpu.device(),
                    vello::RendererOptions {
                        use_cpu: false,
                        antialiasing_support: vello::AaSupport::area_only(),
                        num_init_threads: threads,
                        pipeline_cache: None,
                    },
                )
                .map_err(vello_error)?,
            )),
        };
        Ok(Self { backend, renderer })
    }

    /// The renderer shared by everyone drawing on `gpu`; created on first use and
    /// kept until the context is dropped. While another thread (the warm-up)
    /// builds it, an interactive thread (`bae_render::is_interactive_thread`)
    /// gets a "warming up" error instead of waiting; others wait.
    pub fn shared(gpu: &Arc<GpuContext>) -> Result<Arc<Self>, RenderError> {
        Self::get(gpu, None)
    }

    /// Builds the shared renderer ahead of its first use, compiling on a few
    /// threads (the app's warm-up).
    pub fn warm(gpu: &Arc<GpuContext>) -> Result<(), RenderError> {
        Self::get(gpu, std::num::NonZeroUsize::new(BACKGROUND_COMPILE_THREADS)).map(drop)
    }

    fn get(
        gpu: &Arc<GpuContext>,
        threads: Option<std::num::NonZeroUsize>,
    ) -> Result<Arc<Self>, RenderError> {
        let interactive = bae_render::is_interactive_thread();
        let mut shared = SHARED.lock();
        loop {
            shared.retain(|(g, _)| g.strong_count() > 0);
            match shared
                .iter()
                .find(|(g, _)| std::ptr::eq(g.as_ptr(), Arc::as_ptr(gpu)))
            {
                Some((_, Slot::Ready(r))) => return Ok(r.clone()),
                Some((_, Slot::Building)) if interactive => {
                    return Err(bae_render::warming_up("the vector renderer"));
                }
                Some((_, Slot::Building)) => BUILT.wait(&mut shared),
                None => break,
            }
        }
        shared.push((Arc::downgrade(gpu), Slot::Building));
        drop(shared);
        if interactive {
            // Not even started: build it in the background, never on the UI thread.
            let background = gpu.clone();
            let spawned = std::thread::Builder::new()
                .name("bae-vector-build".into())
                .spawn(move || {
                    bae_gpu::warm_up::lower_priority();
                    let threads = std::num::NonZeroUsize::new(BACKGROUND_COMPILE_THREADS);
                    Self::finish(&background, Self::build_shared(&background, threads)).ok();
                });
            if spawned.is_err() {
                return Self::finish(gpu, Self::build_shared(gpu, None));
            }
            return Err(bae_render::warming_up("the vector renderer"));
        }
        Self::finish(gpu, Self::build_shared(gpu, threads))
    }

    fn build_shared(
        gpu: &GpuContext,
        threads: Option<std::num::NonZeroUsize>,
    ) -> Result<Arc<Self>, RenderError> {
        let r = Arc::new(Self::build(gpu, VectorBackend::for_adapter(gpu), threads)?);
        if r.backend == VectorBackend::Cpu {
            tracing::warn!(
                "vector shapes and text are rasterized on the CPU (adapter {:?} is {:?})",
                gpu.adapter_info().name,
                gpu.adapter_info().device_type
            );
        }
        Ok(r)
    }

    /// Publishes the outcome of a build and wakes the threads waiting for it.
    fn finish(
        gpu: &Arc<GpuContext>,
        built: Result<Arc<Self>, RenderError>,
    ) -> Result<Arc<Self>, RenderError> {
        let mut shared = SHARED.lock();
        let at = shared
            .iter()
            .position(|(g, _)| std::ptr::eq(g.as_ptr(), Arc::as_ptr(gpu)));
        match (&built, at) {
            (Ok(r), Some(i)) => shared[i].1 = Slot::Ready(r.clone()),
            (Err(_), Some(i)) => {
                shared.remove(i);
            }
            (_, None) => {}
        }
        BUILT.notify_all();
        drop(shared);
        // Frames drawn while it was building are incomplete: draw them again.
        bae_gpu::warm_up::request_redraw();
        built
    }

    /// The backend in use.
    pub fn backend(&self) -> VectorBackend {
        self.backend
    }

    fn check_size(gpu: &GpuContext, width: u32, height: u32) -> Result<(), RenderError> {
        let limit = gpu.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(RenderError::TooLarge {
                width,
                height,
                limit,
            });
        }
        Ok(())
    }

    /// Renders `scene` into a new `width × height` [`VELLO_FORMAT`] texture
    /// (straight alpha, transparent background) with Vello on the GPU. The work
    /// is submitted to the queue immediately, so later submissions see the
    /// result. Fails on the CPU backend (use [`VelloRenderer::render_region`]).
    pub fn render(
        &self,
        gpu: &GpuContext,
        scene: &vello::Scene,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, RenderError> {
        let Some(renderer) = &self.renderer else {
            return Err(RenderError::Other(
                "vello GPU rendering is disabled on this adapter (CPU backend)".into(),
            ));
        };
        Self::check_size(gpu, width, height)?;
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("bae-vector"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: VELLO_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        if width == 0 || height == 0 {
            return Ok(texture);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        renderer
            .lock()
            .render_to_texture(
                gpu.device(),
                gpu.queue(),
                scene,
                &view,
                &vello::RenderParams {
                    base_color: Color::TRANSPARENT,
                    width,
                    height,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(vello_error)?;
        Ok(texture)
    }

    /// Rasterizes `width × height` texels with `vello_cpu` and uploads them as a
    /// premultiplied [`VELLO_FORMAT`] texture.
    fn render_cpu(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        draw: impl FnOnce(&mut vello_cpu::RenderContext),
    ) -> Result<wgpu::Texture, RenderError> {
        Self::check_size(gpu, width, height)?;
        let (w, h) = (width.max(1), height.max(1));
        let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
            return Err(RenderError::TooLarge {
                width,
                height,
                limit: u32::from(u16::MAX),
            });
        };
        let mut ctx = vello_cpu::RenderContext::new(w16, h16);
        if width > 0 && height > 0 {
            draw(&mut ctx);
        }
        ctx.flush();
        let mut pixmap = vello_cpu::Pixmap::new(w16, h16);
        ctx.render(&mut pixmap, &mut vello_cpu::Resources::new());
        let texture = bae_gpu::transfer::create_texture_with_data(
            gpu,
            &wgpu::TextureDescriptor {
                label: Some("bae-vector.cpu"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VELLO_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            pixmap.data_as_u8_slice(),
        )?;
        Ok(texture)
    }

    /// Renders the part `roi` (layer pixels) of `draw` at `scale` texels per pixel
    /// as a [`GpuImage`] covering `roi`: straight alpha from Vello on the GPU,
    /// premultiplied from `vello_cpu` on the CPU backend.
    pub fn render_region(
        &self,
        gpu: &GpuContext,
        roi: IRect,
        scale: f64,
        draw: &dyn VectorDraw,
    ) -> Result<GpuImage, RenderError> {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let width = (f64::from(roi.width()) * scale).ceil() as u32;
        let height = (f64::from(roi.height()) * scale).ceil() as u32;
        let transform =
            Affine::scale(scale) * Affine::translate((-f64::from(roi.x0), -f64::from(roi.y0)));
        let (texture, alpha) = match self.backend {
            VectorBackend::Gpu => {
                let mut scene = vello::Scene::new();
                if width > 0 && height > 0 {
                    let viewport = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
                    draw.encode(&mut scene, transform, viewport);
                }
                (
                    self.render(gpu, &scene, width, height)?,
                    ImageAlpha::Straight,
                )
            }
            VectorBackend::Cpu => (
                Self::render_cpu(gpu, width, height, |ctx| draw.draw_cpu(ctx, transform))?,
                ImageAlpha::Premultiplied,
            ),
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(GpuImage::new(texture, view, roi, alpha, 1.0 / scale))
    }
}

/// Integer layer-pixel bounds of a scene (stroke extents and one pixel of
/// anti-aliasing included); empty when nothing is drawn.
pub fn pixel_bounds(scene: &VectorScene) -> IRect {
    match scene.bounds(true) {
        Some(r) if r.is_finite() => {
            let clamp = |v: f64| v.clamp(-(1 << 24) as f64, (1 << 24) as f64);
            IRect {
                x0: clamp(r.x0.floor()) as i32 - 1,
                y0: clamp(r.y0.floor()) as i32 - 1,
                x1: clamp(r.x1.ceil()) as i32 + 1,
                y1: clamp(r.y1.ceil()) as i32 + 1,
            }
        }
        _ => IRect::EMPTY,
    }
}

/// A shape layer (or any [`VectorScene`]) as a bae-render layer source.
#[derive(Debug)]
pub struct ShapeSource {
    scene: Arc<VectorScene>,
    bounds: IRect,
    hash: u64,
    memo: RenderMemo,
    #[cfg(target_os = "macos")]
    shared: Option<Arc<SharedDrawing>>,
}

/// Layer sources by content hash. When it fills up it drops only the entries
/// not used since the last sweep (second chance) and lets the limit follow the
/// working set, so every layer still on screen keeps its source and the image
/// it remembered, however many layers there are.
#[derive(Debug)]
pub struct SourceCache<T> {
    map: FxHashMap<u64, (Arc<T>, bool)>,
    limit: usize,
}

/// Entries a [`SourceCache`] holds before its first sweep.
const CACHE_ENTRIES: usize = 512;

impl<T> Default for SourceCache<T> {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            limit: CACHE_ENTRIES,
        }
    }
}

impl<T> SourceCache<T> {
    /// The source for `hash`, marked as used.
    pub fn get(&mut self, hash: u64) -> Option<Arc<T>> {
        let (source, used) = self.map.get_mut(&hash)?;
        *used = true;
        Some(source.clone())
    }

    /// Adds `source` for `hash`, sweeping first when full.
    pub fn insert(&mut self, hash: u64, source: Arc<T>) {
        if self.map.len() >= self.limit {
            self.map.retain(|_, (_, used)| std::mem::take(used));
            self.limit = (2 * self.map.len()).max(CACHE_ENTRIES);
        }
        self.map.insert(hash, (source, true));
    }
}

/// The last image a vector source rendered, reused while the request is the
/// same: layers that only move, turn or fade are rasterized once, not every
/// frame (the image is only ever read).
#[derive(Debug, Default)]
pub struct RenderMemo(Mutex<Option<MemoEntry>>);

#[derive(Debug)]
struct MemoEntry {
    gpu: usize,
    roi: IRect,
    scale: u64,
    image: GpuImage,
}

impl RenderMemo {
    /// The image for `roi` at `scale` on `gpu`: the remembered one, or `render`'s.
    pub fn get_or_render(
        &self,
        gpu: &Arc<GpuContext>,
        roi: IRect,
        scale: f64,
        render: impl FnOnce() -> Result<GpuImage, RenderError>,
    ) -> Result<GpuImage, RenderError> {
        let gpu = Arc::as_ptr(gpu) as usize;
        if let Some(e) = &*self.0.lock()
            && e.gpu == gpu
            && e.roi == roi
            && e.scale == scale.to_bits()
        {
            return Ok(e.image.clone());
        }
        let image = render()?;
        *self.0.lock() = Some(MemoEntry {
            gpu,
            roi,
            scale: scale.to_bits(),
            image: image.clone(),
        });
        Ok(image)
    }
}

impl ShapeSource {
    /// A source drawing `scene` (layer space).
    pub fn new(scene: Arc<VectorScene>) -> Self {
        Self {
            bounds: pixel_bounds(&scene),
            hash: scene.content_hash(),
            scene,
            memo: RenderMemo::default(),
            #[cfg(target_os = "macos")]
            shared: None,
        }
    }

    /// The display list.
    pub fn scene(&self) -> &Arc<VectorScene> {
        &self.scene
    }

    /// Renders `roi` at `scale` on `gpu` with the shared Vello renderer.
    pub fn render_on(
        &self,
        gpu: &Arc<GpuContext>,
        roi: IRect,
        scale: f64,
    ) -> Result<GpuImage, RenderError> {
        let roi = roi.intersect(&self.bounds);
        self.memo.get_or_render(gpu, roi, scale, || {
            let draw = || VelloRenderer::shared(gpu)?.render_region(gpu, roi, scale, &*self.scene);
            // The per-layer memo above still holds this layer's own scale and
            // ROI when another instance of the drawing asks for a different one.
            #[cfg(target_os = "macos")]
            if let Some(shared) = &self.shared {
                return shared.memo.get_or_render(gpu, roi, scale, draw);
            }
            draw()
        })
    }

    /// Renders `roi` at `scale` with a specific renderer (e.g. a forced backend).
    pub fn render_with(
        &self,
        renderer: &VelloRenderer,
        gpu: &GpuContext,
        roi: IRect,
        scale: f64,
    ) -> Result<GpuImage, RenderError> {
        renderer.render_region(gpu, roi.intersect(&self.bounds), scale, self.scene.as_ref())
    }
}

impl LayerSource for ShapeSource {
    fn size(&self) -> [u32; 2] {
        // `[0, w) × [0, h)` covers the positive quadrant of the content; see
        // `bounds` for the full extent.
        [self.bounds.x1.max(0) as u32, self.bounds.y1.max(0) as u32]
    }

    /// Layer-pixel bounds of the content; may start at negative coordinates
    /// (shapes are drawn around the layer origin).
    fn bounds(&self) -> IRect {
        self.bounds
    }

    fn content_hash(&self, _time: Time) -> u64 {
        self.hash
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        request: &SourceRequest,
    ) -> Result<GpuImage, RenderError> {
        self.render_on(ctx.gpu, request.roi, request.scale)
    }
}

/// Equal display lists can share a raster even when the project paths differ.
/// The provider's index has only weak references, so it cannot pin textures.
#[cfg(target_os = "macos")]
#[derive(Debug)]
struct SharedDrawing {
    scene: Arc<VectorScene>,
    memo: RenderMemo,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
struct SharedDrawings {
    by_hash: FxHashMap<u64, Weak<SharedDrawing>>,
    next_sweep: usize,
}

#[cfg(target_os = "macos")]
impl SharedDrawings {
    fn share(&mut self, mut source: ShapeSource) -> ShapeSource {
        let shared = self
            .by_hash
            .get(&source.hash)
            .and_then(Weak::upgrade)
            .filter(|drawing| drawing.scene == source.scene);
        let shared = shared.unwrap_or_else(|| {
            if self.by_hash.len() >= self.next_sweep {
                self.by_hash
                    .retain(|_, drawing| drawing.strong_count() != 0);
                self.next_sweep = (2 * self.by_hash.len()).max(CACHE_ENTRIES);
            }
            let drawing = Arc::new(SharedDrawing {
                scene: source.scene.clone(),
                memo: RenderMemo::default(),
            });
            self.by_hash.insert(source.hash, Arc::downgrade(&drawing));
            drawing
        });
        source.scene = shared.scene.clone();
        source.shared = Some(shared);
        source
    }
}

/// Creates [`ShapeSource`]s for [`crate::SHAPE_PROVIDER`] content. Sources are
/// cached by `CustomContent::hash` (a small cache: one entry per visible shape
/// layer and frame is typical).
#[derive(Debug, Default)]
pub struct ShapeProvider {
    cache: Mutex<SourceCache<ShapeSource>>,
    #[cfg(target_os = "macos")]
    drawings: Mutex<SharedDrawings>,
    warnings: Mutex<Vec<String>>,
}

impl ShapeProvider {
    /// An empty provider.
    pub fn new() -> Self {
        Self::default()
    }

    /// Problems found while building sources since the last call.
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut self.warnings.lock())
    }

    /// The display list for custom content: an `EvaluatedLayer` from 03 or a
    /// ready `VectorScene`.
    pub fn scene_of(&self, content: &CustomContent) -> Result<Arc<VectorScene>, RenderError> {
        if let Some(scene) = content.data.downcast_ref::<VectorScene>() {
            return Ok(Arc::new(scene.clone()));
        }
        if let Some(layer) = content.data.downcast_ref::<EvaluatedLayer>() {
            let mut warnings = Vec::new();
            let scene = crate::shape_layer_scene(layer, &mut warnings);
            if !warnings.is_empty() {
                let mut all = self.warnings.lock();
                for w in warnings {
                    if !all.contains(&w) {
                        all.push(w);
                    }
                }
            }
            return Ok(Arc::new(scene));
        }
        Err(RenderError::Other(format!(
            "{}: `{}` content must be an EvaluatedLayer or a VectorScene",
            content.prim, content.provider
        )))
    }
}

impl SourceProvider for ShapeProvider {
    fn id(&self) -> &str {
        crate::SHAPE_PROVIDER
    }

    /// The fills' outlines in layer pixels, for Advanced 3D extrusion (04).
    fn outlines(&self, content: &CustomContent) -> Option<Arc<bae_render::source::Outlines>> {
        let scene = self.scene_of(content).ok()?;
        let mut out = bae_render::source::Outlines::default();
        collect_outlines(&scene.items, &mut out);
        Some(Arc::new(out))
    }

    fn source(&self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        if let Some(hit) = self.cache.lock().get(content.hash) {
            return Ok(hit);
        }
        let source = ShapeSource::new(self.scene_of(content)?);
        #[cfg(target_os = "macos")]
        let source = self.drawings.lock().share(source);
        let source = Arc::new(source);
        self.cache.lock().insert(content.hash, source.clone());
        Ok(source)
    }
}

/// Builds the shared vector renderer during the app's warm-up
/// (`bae_gpu::warm_up`), before the first shape or text layer is drawn.
#[derive(Debug, Default)]
pub struct VectorWarmUp;

impl bae_gpu::WarmUp for VectorWarmUp {
    fn name(&self) -> &str {
        "vector renderer"
    }

    /// Right after the fonts (text and shapes need it).
    fn priority(&self) -> u8 {
        1
    }

    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String> {
        VelloRenderer::warm(gpu).map_err(|e| e.to_string())
    }
}

/// Flattened fill outlines of `items` (layer pixels): a gradient fill takes
/// its average stop colour.
fn collect_outlines(items: &[DrawItem], out: &mut bae_render::source::Outlines) {
    for item in items {
        match item {
            DrawItem::Fill(f) => {
                let mut color = match &f.brush {
                    Brush::Solid(c) => *c,
                    Brush::Gradient(g) if !g.stops.is_empty() => {
                        let n = g.stops.len() as f32;
                        g.stops.iter().fold([0.0; 4], |a, s| {
                            std::array::from_fn(|k| a[k] + s.color[k] / n)
                        })
                    }
                    Brush::Gradient(_) => [0.0; 4],
                };
                color[3] *= f.opacity;
                let mut contours: Vec<Vec<[f64; 2]>> = Vec::new();
                kurbo::flatten(f.transform * &f.path, 0.25, |el| match el {
                    kurbo::PathEl::MoveTo(p) => contours.push(vec![[p.x, p.y]]),
                    kurbo::PathEl::LineTo(p) => {
                        if let Some(c) = contours.last_mut() {
                            c.push([p.x, p.y]);
                        }
                    }
                    _ => {}
                });
                out.fills
                    .push(bae_render::source::OutlineFill { color, contours });
            }
            DrawItem::Layer(l) => collect_outlines(&l.items, out),
            DrawItem::Stroke(_) => {}
        }
    }
}
