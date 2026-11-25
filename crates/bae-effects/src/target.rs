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
use bae_render::effect::{LayerDependency, MaskSelect};
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
    /// Input at the requested stage and comp time.
    fn effect_layer(&self, _dependency: &LayerDependency) -> Option<&GpuImage> {
        None
    }
    /// Layer-space coverage of the requested mask or mask stack.
    fn effect_mask(&self, _select: &MaskSelect) -> Option<&GpuImage> {
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

    fn effect_layer(&self, dependency: &LayerDependency) -> Option<&GpuImage> {
        EncodeContext::effect_layer(self, dependency)
    }

    fn effect_mask(&self, select: &MaskSelect) -> Option<&GpuImage> {
        EncodeContext::effect_mask(self, select)
    }

    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        EncodeContext::allocate(self, rect, pixel_scale)
    }
}

/// The renderer's images cover *space* pixels (`rect`) at `pixel_scale`
/// pixels per texel, while built-in effects work in *render* pixels (the
/// request scale: `output_roi`, `output_bounds`, σ × `params.scale`). This
/// runs `effect` with its input and the layer / mask images it requested
/// expressed in render pixels (rect ÷ pixel scale, scale 1) and maps the result
/// back, so reduced resolutions and lower-density layers place it right.
pub(crate) fn encode_in_render_pixels(
    effect: &dyn TargetEffect,
    ctx: &mut EncodeContext<'_>,
    params: &EffectParams<'_>,
    input: &GpuImage,
    output_roi: IRect,
) -> Result<GpuImage, RenderError> {
    let s = input.pixel_scale;
    if s == 1.0 || effect.space_pixels() {
        return effect.encode(ctx, params, input, output_roi);
    }
    let local = |image: &GpuImage| {
        let mut out = image.clone();
        let k = image.pixel_scale;
        let [w, h] = image.texel_size();
        let x0 = (f64::from(image.rect.x0) / k).round() as i32;
        let y0 = (f64::from(image.rect.y0) / k).round() as i32;
        out.rect = IRect::new(x0, y0, x0 + w as i32, y0 + h as i32);
        out.pixel_scale = 1.0;
        out
    };
    let layers = effect
        .layer_dependencies(params)
        .into_iter()
        .filter_map(|d| {
            let image = ctx.effect_layer(&d).map(local)?;
            Some((d, image))
        })
        .collect();
    let masks = effect
        .mask_dependencies(params)
        .into_iter()
        .filter_map(|m| {
            let image = ctx.effect_mask(&m).map(local)?;
            Some((m, image))
        })
        .collect();
    let mut target = RenderPixels { ctx, layers, masks };
    let mut out = effect.encode(&mut target, params, &local(input), output_roi)?;
    let px = |v: i32| (f64::from(v) * s).round() as i32;
    out.rect = IRect::new(
        px(out.rect.x0),
        px(out.rect.y0),
        px(out.rect.x1),
        px(out.rect.y1),
    );
    out.pixel_scale *= s;
    Ok(out)
}

/// The renderer's context with the requested inputs in render pixels.
struct RenderPixels<'c, 'a> {
    ctx: &'c mut EncodeContext<'a>,
    layers: Vec<(LayerDependency, GpuImage)>,
    masks: Vec<(MaskSelect, GpuImage)>,
}

impl EffectTarget for RenderPixels<'_, '_> {
    fn gpu(&self) -> &Arc<GpuContext> {
        self.ctx.gpu
    }

    fn format(&self) -> wgpu::TextureFormat {
        self.ctx.format
    }

    fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.ctx.encoder
    }

    fn effect_input(&self, layer: &PrimPath) -> Option<&GpuImage> {
        self.layers
            .iter()
            .find(|(d, _)| d.layer == *layer)
            .map(|(_, image)| image)
    }

    fn effect_layer(&self, dependency: &LayerDependency) -> Option<&GpuImage> {
        self.layers
            .iter()
            .find(|(d, _)| d == dependency)
            .map(|(_, image)| image)
    }

    fn effect_mask(&self, select: &MaskSelect) -> Option<&GpuImage> {
        self.masks
            .iter()
            .find(|(m, _)| m == select)
            .map(|(_, image)| image)
    }

    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        self.ctx.allocate(rect, pixel_scale)
    }
}

/// An effect that can encode into any [`EffectTarget`]. Every built-in effect
/// and [`crate::DeclarativeEffect`] implements it; `bae_render::Effect::render`
/// forwards here.
pub trait TargetEffect: bae_render::Effect {
    /// The effect maps the renderer's space-pixel images itself
    /// (`image_grid`); otherwise it works in render pixels and
    /// [`encode_in_render_pixels`] converts for it.
    fn space_pixels(&self) -> bool {
        false
    }

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
    layers: Vec<(LayerDependency, GpuImage)>,
    masks: Vec<(MaskSelect, GpuImage)>,
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
            layers: Vec::new(),
            masks: Vec::new(),
            allocations: 0,
        }
    }

    /// Supplies pre-rendered dependency images for a test or benchmark.
    pub fn set_effect_inputs(
        &mut self,
        layers: Vec<(LayerDependency, GpuImage)>,
        masks: Vec<(MaskSelect, GpuImage)>,
    ) {
        self.layers = layers;
        self.masks = masks;
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

    fn effect_input(&self, layer: &PrimPath) -> Option<&GpuImage> {
        self.layers
            .iter()
            .find(|(dependency, _)| dependency.layer == *layer)
            .map(|(_, image)| image)
    }

    fn effect_layer(&self, dependency: &LayerDependency) -> Option<&GpuImage> {
        self.layers
            .iter()
            .find(|(key, _)| key == dependency)
            .map(|(_, image)| image)
    }

    fn effect_mask(&self, select: &MaskSelect) -> Option<&GpuImage> {
        self.masks
            .iter()
            .find(|(key, _)| key == select)
            .map(|(_, image)| image)
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
            ..
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
