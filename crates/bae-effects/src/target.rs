//! Where an effect encodes its work: the renderer's [`EncodeContext`] or a
//! [`Standalone`] target (tests, benchmarks, tools) with its own encoder and
//! texture pool.
//!
//! `bae_render::EncodeContext` cannot be built outside `bae-render` (its
//! allocator is crate-private, request R04-12), so every built-in effect
//! implements [`TargetEffect::encode`] against [`EffectTarget`] and its
//! `Effect::render` forwards to it.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::PrimPath;
use bae_render::{EffectParams, EncodeContext, GpuImage, IRect, ImageAlpha, RenderError};

/// GPU access while an effect is encoded.
pub trait EffectTarget {
    /// The shared GPU context.
    fn gpu(&self) -> &Arc<GpuContext>;
    /// Working texture format.
    fn format(&self) -> wgpu::TextureFormat;
    /// The frame's command encoder.
    fn encoder(&mut self) -> &mut wgpu::CommandEncoder;
    /// Source-stage input requested by this effect, if available.
    fn effect_input(&self, _layer: &PrimPath) -> Option<&GpuImage> {
        None
    }

    /// A working-format image covering `rect` (contents undefined), usable as a
    /// storage texture.
    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError>;
}

impl EffectTarget for EncodeContext<'_> {
    fn gpu(&self) -> &Arc<GpuContext> {
        self.gpu
    }

    fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.encoder
    }

    fn effect_input(&self, layer: &PrimPath) -> Option<&GpuImage> {
        EncodeContext::effect_input(self, layer)
    }

    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        EncodeContext::allocate(self, rect, pixel_scale)
    }
}

/// An effect that can encode into any [`EffectTarget`]. Every built-in effect
/// and [`crate::DeclarativeEffect`] implements it; `bae_render::Effect::render`
/// forwards here.
pub trait TargetEffect: bae_render::Effect {
    /// Same contract as `bae_render::Effect::render`.
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError>;
}

/// Usage of every image a [`Standalone`] target hands out.
pub const IMAGE_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::TEXTURE_BINDING
    .union(wgpu::TextureUsages::STORAGE_BINDING)
    .union(wgpu::TextureUsages::RENDER_ATTACHMENT)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

/// A self-contained target: its own command encoder and a simple texture pool
/// (textures are reused after [`Standalone::submit`] once no image refers to
/// them). For tests, benchmarks and tools; the renderer uses `EncodeContext`.
pub struct Standalone {
    gpu: Arc<GpuContext>,
    format: wgpu::TextureFormat,
    encoder: wgpu::CommandEncoder,
    pool: Vec<(wgpu::Texture, bool)>,
    /// Textures handed out (allocated or reused) since creation.
    pub allocations: usize,
}

impl std::fmt::Debug for Standalone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Standalone")
            .field("format", &self.format)
            .field("pool", &self.pool.len())
            .finish_non_exhaustive()
    }
}

impl Standalone {
    /// A target on `gpu` with working format `format` (RGBA16F or RGBA32F).
    pub fn new(gpu: Arc<GpuContext>, format: wgpu::TextureFormat) -> Self {
        let encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bae.effects.standalone"),
            });
        Self {
            gpu,
            format,
            encoder,
            pool: Vec::new(),
            allocations: 0,
        }
    }

    /// Submits the recorded work and starts a new encoder.
    pub fn submit(&mut self) -> wgpu::SubmissionIndex {
        let encoder = std::mem::replace(
            &mut self.encoder,
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("bae.effects.standalone"),
                }),
        );
        let index = self.gpu.queue().submit([encoder.finish()]);
        for slot in &mut self.pool {
            slot.1 = false;
        }
        index
    }

    /// The command encoder (to record extra work, e.g. timer resolves).
    pub fn encoder_mut(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.encoder
    }

    /// Submits and waits until the GPU is idle.
    pub fn finish(&mut self) -> Result<(), RenderError> {
        self.submit();
        self.gpu
            .wait_idle()
            .map_err(|e| RenderError::Other(e.to_string()))
    }

    /// Wraps an existing texture as an image covering `rect`.
    pub fn image(texture: wgpu::Texture, rect: IRect, alpha: ImageAlpha) -> GpuImage {
        let mut image = GpuImage::from_texture(texture, alpha);
        image.rect = rect;
        image
    }

    /// Runs `effect` on `input` for `output_roi`.
    pub fn render(
        &mut self,
        effect: &dyn TargetEffect,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        effect.encode(self, params, input, output_roi)
    }
}

impl EffectTarget for Standalone {
    fn gpu(&self) -> &Arc<GpuContext> {
        &self.gpu
    }

    fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.encoder
    }

    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        self.allocations += 1;
        alloc(&self.gpu, &mut self.pool, rect, pixel_scale, self.format)
    }
}

impl Standalone {
    /// Runs a declarative effect (it encodes and allocates at the same time,
    /// so it needs the encoder and the pool separately).
    pub fn render_declarative(
        &mut self,
        effect: &bae_plugin::DeclarativeEffect,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let Standalone {
            gpu,
            format,
            encoder,
            pool,
            allocations,
        } = self;
        let gpu = gpu.clone();
        let working = *format;
        let mut allocate = |rect: IRect, scale: f64, f: wgpu::TextureFormat| {
            *allocations += 1;
            alloc(&gpu, pool, rect, scale, f)
        };
        effect.render_with(
            &gpu,
            encoder,
            working,
            &mut allocate,
            params,
            input,
            output_roi,
        )
    }
}

fn alloc(
    gpu: &GpuContext,
    pool: &mut Vec<(wgpu::Texture, bool)>,
    rect: IRect,
    pixel_scale: f64,
    format: wgpu::TextureFormat,
) -> Result<GpuImage, RenderError> {
    let [w, h] = [rect.width().max(1), rect.height().max(1)];
    let limit = gpu.limits().max_texture_dimension_2d;
    if w > limit || h > limit {
        return Err(RenderError::TooLarge {
            width: w,
            height: h,
            limit,
        });
    }
    // Reuse a texture of the exact size that is free in this batch (textures are
    // marked free again by `Standalone::submit`).
    let texture = match pool
        .iter_mut()
        .find(|(t, used)| !*used && t.width() == w && t.height() == h && t.format() == format)
    {
        Some(slot) => {
            slot.1 = true;
            slot.0.clone()
        }
        None => {
            let t = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("bae.effects.standalone.image"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: IMAGE_USAGE,
                view_formats: &[],
            });
            pool.push((t.clone(), true));
            t
        }
    };
    let mut image = GpuImage::from_texture(texture, ImageAlpha::Premultiplied);
    image.rect = rect;
    image.pixel_scale = pixel_scale;
    Ok(image)
}
