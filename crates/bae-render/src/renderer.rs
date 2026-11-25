//! The renderer: builds and executes render graphs on the shared GPU context,
//! caches frames, and reads frames back for export.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bae_gpu::transfer::ImageData;
use bae_gpu::{GpuContext, wgpu};
use bae_model::{PrimPath, StageEvent, Time};

use crate::cache::{FrameCache, FrameKey};
use crate::effect::EffectLibrary;
use crate::error::RenderError;
use crate::exec::{DrawBuffer, MaskBuffers, execute};
use crate::footage::FootageCache;
use crate::gpu::Pipelines;
use crate::graph::{self, RenderGraph, SourceResolver};
use crate::input::{CustomContent, FootageRef, RenderComp};
use crate::pool::{PoolStats, TexturePool};
use crate::request::{RenderRequest, WorkFormat};
use crate::source::{GpuImage, LayerSource, SourceProviders};

/// Renderer settings.
#[derive(Clone, Copy, Debug)]
pub struct RendererConfig {
    /// Frame cache budget in bytes (0 disables the cache).
    pub frame_cache_bytes: u64,
    /// Idle bytes the texture pool keeps for reuse.
    pub pool_idle_bytes: u64,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            frame_cache_bytes: 2 << 30,
            pool_idle_bytes: 512 << 20,
        }
    }
}

/// Timing and size of one rendered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// Graph nodes.
    pub nodes: usize,
    /// Layer draws (ops) over all composites.
    pub draws: usize,
    /// GPU draw calls after batching.
    pub draw_calls: usize,
    /// Render passes.
    pub passes: usize,
    /// CPU time building the graph.
    pub build: Duration,
    /// CPU time encoding and submitting.
    pub encode: Duration,
}

/// A rendered frame: a premultiplied working-format image of the comp (or of
/// the requested ROI), in comp texels at the request resolution.
#[derive(Clone, Debug)]
pub struct RenderedFrame {
    /// The image. `image.rect` is in comp pixels; at reduced resolution each texel
    /// covers `pixel_scale` pixels ([`GpuImage::texel_size`] texels).
    pub image: GpuImage,
    /// The comp.
    pub comp: PrimPath,
    /// Comp time.
    pub time: Time,
    /// Served from the frame cache.
    pub from_cache: bool,
    /// Statistics (zero for cached frames).
    pub stats: FrameStats,
    /// Things that were not rendered faithfully.
    pub warnings: Vec<String>,
    /// Something was still warming up and is missing (a vector or text layer,
    /// an effect): the frame is not cached and should be drawn again when the
    /// warm-up reports progress (`bae_gpu::warm_up::request_redraw`).
    pub incomplete: bool,
    /// CPU time spent on each top-level layer of the comp, in drawing order,
    /// when [`Renderer::set_layer_times`] is on (AE's Render Time column);
    /// empty otherwise and for cached frames. Drawing the layer into the comp
    /// itself is not included (it is one batched pass for all layers).
    pub layer_times: Vec<(PrimPath, Duration)>,
    /// GPU time per top-level layer, the newest measurement read back (from
    /// a frame or two earlier), when the device has timestamp queries inside
    /// encoders; empty otherwise.
    pub layer_gpu_times: Vec<(PrimPath, Duration)>,
}

/// How [`Renderer::read_rgba8`] treats alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OutputAlpha {
    /// Straight (unassociated) alpha, as PNG expects.
    #[default]
    Straight,
    /// Premultiplied alpha.
    Premultiplied,
    /// Composited over a background colour (straight RGB), opaque.
    Over([f32; 3]),
}

/// Size of the output pass's uniform block.
pub(crate) const OUTPUT_PARAMS_SIZE: u64 = 32;

/// The output pass's uniform block (`output.wgsl` `Params`). `float`: the
/// target is a float format, so colour outside `[0, alpha]` is kept.
pub(crate) fn output_params(alpha: OutputAlpha, float: bool) -> [u8; OUTPUT_PARAMS_SIZE as usize] {
    let (mode, bg) = match alpha {
        OutputAlpha::Premultiplied => (0u32, [0.0f32; 3]),
        OutputAlpha::Straight => (1, [0.0; 3]),
        OutputAlpha::Over(bg) => (2, bg),
    };
    let mut params = [0u8; OUTPUT_PARAMS_SIZE as usize];
    params[..12].copy_from_slice(bytemuck::cast_slice(&bg));
    params[16..20].copy_from_slice(&mode.to_ne_bytes());
    params[20..24].copy_from_slice(&u32::from(float).to_ne_bytes());
    params
}

/// Exact-sized display output. Keep the view with the texture so repeated
/// presentations never sample the padding of a working texture.
pub struct OutputTarget(wgpu::TextureView);

impl OutputTarget {
    /// The persistent sampled view used by the viewer registration.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.0
    }

    /// Exact pixel dimensions of this RGBA8 display target.
    pub fn size(&self) -> [u32; 2] {
        [self.0.texture().width(), self.0.texture().height()]
    }
}

pub(crate) struct DisplayOutput {
    available: Option<OutputTarget>,
    uniform: wgpu::Buffer,
}

impl DisplayOutput {
    /// Records the conversion of `image`. An `incomplete` frame (something
    /// still warming up) is never shown, so an available target comes back
    /// untouched: it may still be on screen with the last complete frame.
    pub(crate) fn record(
        self,
        device: &wgpu::Device,
        pipelines: &mut Pipelines,
        encoder: &mut wgpu::CommandEncoder,
        image: &GpuImage,
        incomplete: bool,
    ) -> OutputTarget {
        if incomplete && let Some(target) = self.available {
            return target;
        }
        let [w, h] = image.texel_size();
        let target = self
            .available
            .filter(|t| t.size() == [w.max(1), h.max(1)])
            .unwrap_or_else(|| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("bae-render.output"),
                    size: wgpu::Extent3d {
                        width: w.max(1),
                        height: h.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                OutputTarget(view)
            });
        crate::exec::record_output_pass(
            device,
            pipelines,
            encoder,
            image,
            target.view(),
            wgpu::TextureFormat::Rgba8Unorm,
            &self.uniform,
        );
        target
    }
}

/// The GPU compositor.
pub struct Renderer {
    gpu: Arc<GpuContext>,
    pipelines: Pipelines,
    pool: TexturePool,
    draws: DrawBuffer,
    output_uniform: Option<(OutputAlpha, wgpu::Buffer)>,
    readback_target: Option<OutputTarget>,
    mask_buffers: MaskBuffers,
    frames: FrameCache<GpuImage>,
    footage: FootageCache,
    providers: SourceProviders,
    effects: EffectLibrary,
    /// Measure per-layer render times ([`RenderedFrame::layer_times`]).
    layer_times: bool,
    /// Timestamp queries for per-layer GPU times (created when needed).
    gpu_timer: Option<crate::gpu_timer::GpuTimer>,
    /// The newest per-layer GPU times read back.
    gpu_times: Vec<(PrimPath, Duration)>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("gpu", &self.gpu.summary())
            .field("pool", &self.pool)
            .field("frames", &self.frames.stats())
            .finish_non_exhaustive()
    }
}

struct Resolver<'a> {
    footage: &'a mut FootageCache,
    providers: &'a SourceProviders,
    warnings: Vec<String>,
}

impl SourceResolver for Resolver<'_> {
    fn footage(
        &mut self,
        footage: &FootageRef,
    ) -> Result<Option<Arc<dyn LayerSource>>, RenderError> {
        let Some(file) = &footage.file else {
            return Ok(None);
        };
        match self.footage.get(file, footage.sequence) {
            Ok(source) => Ok(Some(source)),
            Err(message) => {
                self.warnings.push(message);
                Ok(None)
            }
        }
    }

    fn custom(&mut self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        self.providers.resolve(content)
    }
}

impl Renderer {
    /// A renderer on the shared GPU context with default settings.
    pub fn new(gpu: Arc<GpuContext>) -> Self {
        Self::with_config(gpu, RendererConfig::default())
    }

    /// A renderer with explicit settings.
    pub fn with_config(gpu: Arc<GpuContext>, config: RendererConfig) -> Self {
        let pipelines = Pipelines::new(gpu.device());
        Self {
            pipelines,
            pool: TexturePool::new(config.pool_idle_bytes),
            draws: DrawBuffer::default(),
            output_uniform: None,
            readback_target: None,
            mask_buffers: MaskBuffers::default(),
            frames: FrameCache::new(config.frame_cache_bytes),
            footage: FootageCache::new(gpu.clone()),
            providers: SourceProviders::new(),
            effects: EffectLibrary::new(),
            layer_times: false,
            gpu_timer: None,
            gpu_times: Vec::new(),
            gpu,
        }
    }

    /// Measures how long each top-level layer takes to render (AE's Render
    /// Time column, [`RenderedFrame::layer_times`]). Off by default: it times
    /// every graph node.
    pub fn set_layer_times(&mut self, on: bool) {
        self.layer_times = on;
        if on && self.gpu_timer.is_none() {
            self.gpu_timer = crate::gpu_timer::GpuTimer::new(self.gpu.device());
        }
        if !on {
            self.gpu_timer = None;
            self.gpu_times.clear();
        }
    }

    /// The shared GPU context.
    pub fn gpu(&self) -> &Arc<GpuContext> {
        &self.gpu
    }

    /// Hash the pixels with camera/light dependencies from the installed effects.
    pub fn content_hash(&self, comp: &RenderComp) -> u64 {
        use std::hash::Hasher;
        let mut h = rustc_hash::FxHasher::default();
        comp.hash_content(&mut h, Some(&self.effects));
        h.finish()
    }

    /// Effect implementations (register built-in and plugin effects here).
    pub fn effects_mut(&mut self) -> &mut EffectLibrary {
        &mut self.effects
    }

    /// Takes the effects of the app's `Registry<dyn Effect>` (built-in and
    /// plugin effects, see [`EffectLibrary::sync`]); cached frames are dropped
    /// when the set changed. Cheap when nothing changed: call every frame.
    pub fn sync_effects(&mut self, registry: &bae_app::Registry<dyn crate::Effect>) -> bool {
        let changed = self.effects.sync(registry);
        if changed {
            self.frames.clear();
        }
        changed
    }

    /// Layer source providers (05's vectors and text, plugin generators).
    pub fn providers_mut(&mut self) -> &mut SourceProviders {
        &mut self.providers
    }

    /// The frame cache (statistics, cached times for the timeline).
    pub fn frame_cache(&self) -> &FrameCache<GpuImage> {
        &self.frames
    }

    /// Changes the frame cache budget (0 disables caching).
    pub fn set_frame_cache_budget(&mut self, bytes: u64) {
        self.frames.set_budget(bytes);
    }

    /// Texture pool statistics.
    pub fn pool_stats(&self) -> PoolStats {
        self.pool.stats()
    }

    /// Times of `comp` with a cached frame (the timeline's green bar).
    pub fn cached_frames(&self, comp: &PrimPath) -> Vec<Time> {
        self.frames.cached_times(comp)
    }

    /// Drops cached frames affected by `event`; returns how many.
    pub fn invalidate(&mut self, event: &StageEvent) -> usize {
        self.frames.invalidate(event)
    }

    /// Shares the app's media library (`bae_media::MediaModule` resource) so
    /// footage frames are decoded and cached once for every consumer.
    #[cfg(feature = "media")]
    pub fn set_media_library(&mut self, library: Arc<bae_media::MediaLibrary>) {
        self.footage.set_library(library);
        self.frames.clear();
    }

    /// Closes every open footage file so the next frame reopens them (after
    /// files changed on disk or assets were relinked).
    pub fn reload_footage(&mut self) {
        self.footage.clear();
        self.frames.clear();
    }

    /// The working format actually used for `request` (RGBA32F falls back to
    /// RGBA16F when the device cannot filter and blend 32-bit floats).
    pub fn work_format(&self, request: &RenderRequest) -> wgpu::TextureFormat {
        let needed = wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::FLOAT32_BLENDABLE;
        match request.format {
            WorkFormat::Rgba32Float if self.gpu.features().contains(needed) => {
                wgpu::TextureFormat::Rgba32Float
            }
            _ => wgpu::TextureFormat::Rgba16Float,
        }
    }

    /// Renders `comp` (already evaluated at its time) under `request`.
    pub fn render_frame(
        &mut self,
        comp: &RenderComp,
        request: &RenderRequest,
    ) -> Result<RenderedFrame, RenderError> {
        self.render_frame_with(comp, request, None)
    }

    /// [`Renderer::render_frame`] with other frames of the comp for effects
    /// that read them (`crate::eval::EvaluatorFrames`). Such frames are not
    /// kept in the frame cache.
    pub fn render_frame_with(
        &mut self,
        comp: &RenderComp,
        request: &RenderRequest,
        frames: Option<&mut dyn crate::graph::CompFrames>,
    ) -> Result<RenderedFrame, RenderError> {
        Ok(self.render(comp, request, frames, None)?.0)
    }

    /// [`Renderer::render_frame_with`] followed by [`Renderer::output_texture`]
    /// in one GPU submission (the viewer path). When the frame is incomplete,
    /// `available` comes back untouched (still holding the frame shown).
    pub fn render_frame_output(
        &mut self,
        comp: &RenderComp,
        request: &RenderRequest,
        frames: Option<&mut dyn crate::graph::CompFrames>,
        alpha: OutputAlpha,
        available: Option<OutputTarget>,
    ) -> Result<(RenderedFrame, OutputTarget), RenderError> {
        let output = self.display_output(alpha, available);
        let (frame, target) = self.render(comp, request, frames, Some(output))?;
        Ok((frame, target.expect("display output requested")))
    }

    fn render(
        &mut self,
        comp: &RenderComp,
        request: &RenderRequest,
        frames: Option<&mut dyn crate::graph::CompFrames>,
        output: Option<DisplayOutput>,
    ) -> Result<(RenderedFrame, Option<OutputTarget>), RenderError> {
        let lookup = (self.frames.stats().budget > 0).then(|| {
            (
                FrameKey {
                    comp: comp.path.clone(),
                    time: comp.time,
                    request: request.fingerprint(),
                },
                self.content_hash(comp),
            )
        });
        if let Some((key, hash)) = &lookup
            && self.frames.content_hash(key) == Some(*hash)
            && let Some(image) = self.frames.get(key)
        {
            let image = image.clone();
            let output = output.map(|out| self.submit_output(&image, out));
            return Ok((
                RenderedFrame {
                    image,
                    comp: comp.path.clone(),
                    time: comp.time,
                    from_cache: true,
                    stats: FrameStats::default(),
                    warnings: Vec::new(),
                    incomplete: false,
                    layer_times: Vec::new(),
                    layer_gpu_times: Vec::new(),
                },
                output,
            ));
        }

        let started = Instant::now();
        let mut resolver = Resolver {
            footage: &mut self.footage,
            providers: &self.providers,
            warnings: Vec::new(),
        };
        let graph = graph::build_with(comp, request, &mut resolver, &self.effects, frames)?;
        let mut warnings = resolver.warnings;
        warnings.extend(graph.warnings.iter().cloned());
        let build = started.elapsed();

        let format = self.work_format(request);
        if request.format == WorkFormat::Rgba32Float && format != wgpu::TextureFormat::Rgba32Float {
            warnings.push("RGBA32F is not filterable/blendable on this GPU; using RGBA16F".into());
        }
        let encode_start = Instant::now();
        let tiles = request.roi.map(|_| crate::exec::TileCache {
            frames: &mut self.frames,
            request: RenderRequest {
                roi: None,
                ..*request
            }
            .fingerprint(),
            comp,
        });
        let executed = execute(
            &self.gpu,
            &mut self.pipelines,
            &mut self.pool,
            &mut self.draws,
            &mut self.mask_buffers,
            &graph,
            format,
            tiles,
            crate::exec::Timing {
                cpu: self.layer_times,
                gpu: self.gpu_timer.as_mut(),
            },
            output,
        )?;
        if let Some(timer) = &mut self.gpu_timer {
            let period = self.gpu.queue().get_timestamp_period();
            if let Some(times) = timer.collect(self.gpu.device(), period) {
                self.gpu_times = times;
            }
        }
        let encode = encode_start.elapsed();
        self.pool.end_frame();

        let output = executed.output;
        let image = executed.image;
        let incomplete = graph.incomplete || executed.incomplete;
        if !graph.time_dependent
            && !incomplete
            && image.byte_size() <= self.frames.stats().budget
            && let Some((key, hash)) = lookup
        {
            self.frames.insert(
                key,
                image.clone(),
                image.byte_size(),
                comp.dependencies(),
                hash,
            );
        }
        for w in &warnings {
            tracing::debug!(comp = %comp.path, "{w}");
        }
        Ok((
            RenderedFrame {
                image,
                comp: comp.path.clone(),
                time: comp.time,
                from_cache: false,
                stats: FrameStats {
                    nodes: graph.nodes.len(),
                    draws: graph.draw_count(),
                    draw_calls: executed.draw_calls,
                    passes: executed.passes,
                    build,
                    encode,
                },
                warnings,
                incomplete,
                layer_times: layer_times(&graph, &executed.node_times),
                layer_gpu_times: self.gpu_times.clone(),
            },
            output,
        ))
    }

    /// Blocks until the GPU has finished all submitted work.
    pub fn wait_idle(&self) -> Result<(), RenderError> {
        Ok(self.gpu.wait_idle()?)
    }

    /// Converts a frame to 8-bit RGBA on the GPU and reads it back (blocking).
    pub fn read_rgba8(
        &mut self,
        frame: &RenderedFrame,
        alpha: OutputAlpha,
    ) -> Result<ImageData, RenderError> {
        let available = self.readback_target.take();
        let target = self.output_texture(frame, alpha, available)?;
        let pixels = bae_gpu::transfer::read_texture(&self.gpu, target.view().texture());
        self.readback_target = Some(target);
        Ok(pixels?)
    }

    /// Converts to RGBA8. Return the display target here only after its consumers have released it.
    pub fn output_texture(
        &mut self,
        frame: &RenderedFrame,
        alpha: OutputAlpha,
        available: Option<OutputTarget>,
    ) -> Result<OutputTarget, RenderError> {
        let out = self.display_output(alpha, available);
        Ok(self.submit_output(&frame.image, out))
    }

    fn submit_output(&mut self, image: &GpuImage, output: DisplayOutput) -> OutputTarget {
        let device = self.gpu.device();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae-render.output"),
        });
        let target = output.record(device, &mut self.pipelines, &mut encoder, image, false);
        self.gpu.queue().submit([encoder.finish()]);
        target
    }

    fn display_output(
        &mut self,
        alpha: OutputAlpha,
        available: Option<OutputTarget>,
    ) -> DisplayOutput {
        let device = self.gpu.device();
        let (previous, uniform) = self.output_uniform.get_or_insert_with(|| {
            use wgpu::util::DeviceExt;
            (
                alpha,
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("bae-render.output"),
                    contents: &output_params(alpha, false),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                }),
            )
        });
        if *previous != alpha {
            self.gpu
                .queue()
                .write_buffer(uniform, 0, &output_params(alpha, false));
            *previous = alpha;
        }
        DisplayOutput {
            available,
            uniform: uniform.clone(),
        }
    }

    /// Records the output conversion of `frame` into `target` (a `format`
    /// render target at least the frame's texel size) with the parameters in
    /// `uniform` ([`output_params`]). Export uses this with reused targets.
    pub(crate) fn record_output(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &RenderedFrame,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        uniform: &wgpu::Buffer,
    ) {
        crate::exec::record_output_pass(
            self.gpu.device(),
            &mut self.pipelines,
            encoder,
            &frame.image,
            target,
            format,
            uniform,
        );
    }

    /// Writes a frame as an 8-bit RGBA PNG (straight alpha unless `alpha` says
    /// otherwise).
    pub fn write_png(
        &mut self,
        frame: &RenderedFrame,
        path: &Path,
        alpha: OutputAlpha,
    ) -> Result<(), RenderError> {
        let image = self.read_rgba8(frame, alpha)?;
        crate::png_out::write_rgba8(path, &image)
    }
}

/// Node times added up per top-level layer, in drawing order.
fn layer_times(graph: &RenderGraph, times: &[Duration]) -> Vec<(PrimPath, Duration)> {
    let mut out: Vec<(PrimPath, Duration)> = Vec::new();
    for (owner, time) in graph.owners.iter().zip(times) {
        let Some(layer) = owner else { continue };
        match out.iter_mut().find(|(l, _)| l == layer) {
            Some((_, total)) => *total += *time,
            None => out.push((layer.clone(), *time)),
        }
    }
    out
}
