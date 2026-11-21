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
use parking_lot::Mutex;
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

type SharedRenderers = Mutex<Vec<(Weak<GpuContext>, Weak<VelloRenderer>)>>;
static SHARED: SharedRenderers = Mutex::new(Vec::new());

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
        let renderer = match backend {
            VectorBackend::Cpu => None,
            VectorBackend::Gpu => Some(Mutex::new(
                vello::Renderer::new(
                    gpu.device(),
                    vello::RendererOptions {
                        use_cpu: false,
                        antialiasing_support: vello::AaSupport::area_only(),
                        num_init_threads: None,
                        pipeline_cache: None,
                    },
                )
                .map_err(vello_error)?,
            )),
        };
        Ok(Self { backend, renderer })
    }

    /// The renderer shared by everyone drawing on `gpu`; created on first use and
    /// dropped when the last user lets go of it.
    pub fn shared(gpu: &Arc<GpuContext>) -> Result<Arc<Self>, RenderError> {
        let mut shared = SHARED.lock();
        shared.retain(|(g, r)| g.strong_count() > 0 && r.strong_count() > 0);
        if let Some(r) = shared
            .iter()
            .find(|(g, _)| std::ptr::eq(g.as_ptr(), Arc::as_ptr(gpu)))
            .and_then(|(_, r)| r.upgrade())
        {
            return Ok(r);
        }
        let r = Arc::new(Self::new(gpu)?);
        if r.backend == VectorBackend::Cpu {
            tracing::warn!(
                "vector shapes and text are rasterized on the CPU (adapter {:?} is {:?})",
                gpu.adapter_info().name,
                gpu.adapter_info().device_type
            );
        }
        shared.push((Arc::downgrade(gpu), Arc::downgrade(&r)));
        Ok(r)
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
}

impl ShapeSource {
    /// A source drawing `scene` (layer space).
    pub fn new(scene: Arc<VectorScene>) -> Self {
        Self {
            bounds: pixel_bounds(&scene),
            hash: scene.content_hash(),
            scene,
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
        VelloRenderer::shared(gpu)?.render_region(gpu, roi, scale, self.scene.as_ref())
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

/// Creates [`ShapeSource`]s for [`crate::SHAPE_PROVIDER`] content. Sources are
/// cached by `CustomContent::hash` (a small cache: one entry per visible shape
/// layer and frame is typical).
#[derive(Debug, Default)]
pub struct ShapeProvider {
    cache: Mutex<FxHashMap<u64, Arc<ShapeSource>>>,
    warnings: Mutex<Vec<String>>,
}

/// Cached sources kept before the cache is cleared.
const CACHE_ENTRIES: usize = 512;

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

    fn source(&self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        if let Some(hit) = self.cache.lock().get(&content.hash) {
            return Ok(hit.clone());
        }
        let source = Arc::new(ShapeSource::new(self.scene_of(content)?));
        let mut cache = self.cache.lock();
        if cache.len() >= CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(content.hash, source.clone());
        Ok(source)
    }
}
