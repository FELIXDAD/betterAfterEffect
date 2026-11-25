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
use crate::graph::{self, SourceResolver};
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

/// The GPU compositor.
pub struct Renderer {
    gpu: Arc<GpuContext>,
    pipelines: Pipelines,
    pool: TexturePool,
    draws: DrawBuffer,
    mask_buffers: MaskBuffers,
    frames: FrameCache<GpuImage>,
    footage: FootageCache,
    providers: SourceProviders,
    effects: EffectLibrary,
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
            mask_buffers: MaskBuffers::default(),
            frames: FrameCache::new(config.frame_cache_bytes),
            footage: FootageCache::new(gpu.clone()),
            providers: SourceProviders::new(),
            effects: EffectLibrary::new(),
            gpu,
        }
    }

    /// The shared GPU context.
    pub fn gpu(&self) -> &Arc<GpuContext> {
        &self.gpu
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
        let key = FrameKey {
            comp: comp.path.clone(),
            time: comp.time,
            request: request.fingerprint(),
        };
        let hash = comp.content_hash();
        if self.frames.content_hash(&key) == Some(hash)
            && let Some(image) = self.frames.get(&key)
        {
            return Ok(RenderedFrame {
                image: image.clone(),
                comp: comp.path.clone(),
                time: comp.time,
                from_cache: true,
                stats: FrameStats::default(),
                warnings: Vec::new(),
                incomplete: false,
            });
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
        let executed = execute(
            &self.gpu,
            &mut self.pipelines,
            &mut self.pool,
            &mut self.draws,
            &mut self.mask_buffers,
            &graph,
            format,
        )?;
        let encode = encode_start.elapsed();
        self.pool.end_frame();

        let image = executed.image;
        let incomplete = graph.incomplete || executed.incomplete;
        if !graph.time_dependent && !incomplete {
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
        Ok(RenderedFrame {
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
        })
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
        let target = self.output_texture(frame, alpha)?;
        Ok(bae_gpu::transfer::read_texture(&self.gpu, &target)?)
    }

    /// Converts a frame to an 8-bit RGBA texture on the GPU, without reading it
    /// back (what [`Renderer::read_rgba8`] reads): `Rgba8Unorm`, usable as a copy
    /// source and as a sampled texture, e.g. published to the viewport registry for
    /// a viewer. The work is submitted; the texture is ready for later commands.
    pub fn output_texture(
        &mut self,
        frame: &RenderedFrame,
        alpha: OutputAlpha,
    ) -> Result<wgpu::Texture, RenderError> {
        let device = self.gpu.device();
        let [w, h] = frame.image.texel_size();
        let target = device.create_texture(&wgpu::TextureDescriptor {
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
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae-render.output"),
            size: OUTPUT_PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.gpu
            .queue()
            .write_buffer(&uniform, 0, &output_params(alpha, false));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae-render.output"),
        });
        self.record_output(
            &mut encoder,
            frame,
            &view,
            wgpu::TextureFormat::Rgba8Unorm,
            &uniform,
        );
        self.gpu.queue().submit([encoder.finish()]);
        Ok(target)
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
        let device = self.gpu.device();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.output"),
            layout: &self.pipelines.output_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&frame.image.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let [w, h] = frame.image.texel_size();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bae-render.output"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(self.pipelines.output_for(device, format));
        pass.set_bind_group(0, &group, &[]);
        pass.set_scissor_rect(0, 0, w.max(1), h.max(1));
        pass.draw(0..3, 0..1);
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
