//! Layer sources: the one interface through which pixels enter a comp.
//!
//! Footage (08, `bae-media`), vector shapes and text (05) and plugin generators
//! all implement [`LayerSource`]. A source renders (or hands over) a GPU image
//! of its layer-space pixels for a requested region; the compositor transforms,
//! masks, mattes and blends it. Solid layers and nested comps are handled inside
//! the render graph and do not go through this trait.
//!
//! [`SourceProvider`]s turn [`CustomContent`] (evaluated, provider-specific data
//! carried by a [`crate::RenderLayer`]) into sources. Built-in and plugin
//! providers register the same way.

use std::fmt;
use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::{PrimPath, Time};
use rustc_hash::FxHashMap;

use crate::effect::{LayerDependency, MaskSelect};
use crate::error::RenderError;
use crate::geom::IRect;
use crate::input::CustomContent;

/// Alpha representation of a [`GpuImage`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageAlpha {
    /// Colour premultiplied by alpha (all working buffers).
    #[default]
    Premultiplied,
    /// Straight colour; the compositor premultiplies when sampling.
    Straight,
    /// Alpha is ignored (treated as 1).
    Opaque,
}

/// A texture on the shared device and the pixel rectangle it represents.
///
/// `rect` is in pixels of the image's own space (layer space for sources, comp
/// space for composites). Texel `(0, 0)` starts at pixel `(rect.x0, rect.y0)` and
/// every texel is `pixel_scale` pixels wide (1 at full resolution, 2 at half, 0.25
/// for a vector layer rasterized at 4× density), so the image has
/// [`GpuImage::texel_size`] texels. The texture may be larger (pooled textures are
/// bucketed); only those texels are defined.
#[derive(Clone, Debug)]
pub struct GpuImage {
    /// The texture (needs `TEXTURE_BINDING`).
    pub texture: wgpu::Texture,
    /// A view of the texture.
    pub view: wgpu::TextureView,
    /// Covered region, in pixels of the image's space.
    pub rect: IRect,
    /// Alpha representation.
    pub alpha: ImageAlpha,
    /// Space pixels per texel (1 at full resolution, 2 at half …).
    pub pixel_scale: f64,
    /// Pool lease: the pooled texture is reused once every clone is dropped.
    lease: Option<Arc<()>>,
}

impl GpuImage {
    /// Wraps a whole texture covering `[0, w) × [0, h)` at scale 1.
    pub fn from_texture(texture: wgpu::Texture, alpha: ImageAlpha) -> Self {
        let rect = IRect::from_size(texture.width(), texture.height());
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self::new(texture, view, rect, alpha, 1.0)
    }

    /// An image from its parts.
    pub fn new(
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        rect: IRect,
        alpha: ImageAlpha,
        pixel_scale: f64,
    ) -> Self {
        Self {
            texture,
            view,
            rect,
            alpha,
            pixel_scale,
            lease: None,
        }
    }

    pub(crate) fn pooled(
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        rect: IRect,
        alpha: ImageAlpha,
        pixel_scale: f64,
        lease: Arc<()>,
    ) -> Self {
        Self {
            lease: Some(lease),
            ..Self::new(texture, view, rect, alpha, pixel_scale)
        }
    }

    /// True when the texture belongs to the renderer's pool.
    pub fn is_pooled(&self) -> bool {
        self.lease.is_some()
    }

    /// Texels that hold the image: `rect` divided by `pixel_scale`, rounded up.
    pub fn texel_size(&self) -> [u32; 2] {
        texels_for(self.rect, self.pixel_scale)
    }

    /// Texture size in texels (may exceed [`GpuImage::texel_size`]).
    pub fn texture_size(&self) -> [u32; 2] {
        [self.texture.width(), self.texture.height()]
    }

    /// GPU bytes of the texture.
    pub fn byte_size(&self) -> u64 {
        let bpp = self.texture.format().block_copy_size(None).unwrap_or(8);
        u64::from(self.texture.width()) * u64::from(self.texture.height()) * u64::from(bpp)
    }
}

/// Texels needed to cover `rect` (pixels) at `pixel_scale` pixels per texel.
pub fn texels_for(rect: IRect, pixel_scale: f64) -> [u32; 2] {
    let s = if pixel_scale.is_finite() && pixel_scale > 0.0 {
        pixel_scale
    } else {
        1.0
    };
    // A tiny epsilon keeps exact multiples (1080 / 2) from rounding up.
    let n = |v: u32| ((f64::from(v) / s) - 1e-9).ceil().max(0.0) as u32;
    [n(rect.width()), n(rect.height())]
}

/// What a source is asked to produce.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceRequest {
    /// Needed region in layer pixels (already clipped to the source bounds).
    pub roi: IRect,
    /// Wanted density in texels per layer pixel: the layer's on-screen
    /// magnification (preview resolution × scale through transform and parents),
    /// so vector/text sources can rasterize at their final size (AE's continuous
    /// rasterization). Raster sources (footage) may ignore it and return their
    /// native resolution; the compositor resamples whatever comes back.
    pub scale: f64,
    /// Time inside the source (footage time, layer time for generators).
    pub time: Time,
}

/// GPU access for sources and effects while a frame is being encoded.
pub struct EncodeContext<'a> {
    /// The shared GPU context.
    pub gpu: &'a Arc<GpuContext>,
    /// The frame's command encoder (work is submitted with the frame).
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// Working texture format.
    pub format: wgpu::TextureFormat,
    pub(crate) allocator: &'a mut dyn ImageAllocator,
    pub(crate) effect_inputs: &'a [(LayerDependency, GpuImage)],
    pub(crate) effect_masks: &'a [(MaskSelect, GpuImage)],
}

impl EncodeContext<'_> {
    /// The first image of `layer` the current effect requested
    /// ([`crate::Effect::layer_dependencies`]). `None` when the layer has no
    /// pixels then (outside its in / out points, missing footage).
    pub fn effect_input(&self, layer: &PrimPath) -> Option<&GpuImage> {
        self.effect_inputs
            .iter()
            .find(|(d, _)| d.layer == *layer)
            .map(|(_, image)| image)
    }

    /// The image of exactly `dependency` (layer, time and stage).
    pub fn effect_layer(&self, dependency: &LayerDependency) -> Option<&GpuImage> {
        self.effect_inputs
            .iter()
            .find(|(d, _)| d == dependency)
            .map(|(_, image)| image)
    }

    /// The coverage image (alpha) of `masks` of the effect's layer
    /// ([`crate::Effect::mask_dependencies`]).
    pub fn effect_mask(&self, masks: &MaskSelect) -> Option<&GpuImage> {
        self.effect_masks
            .iter()
            .find(|(m, _)| m == masks)
            .map(|(_, image)| image)
    }

    /// A working-format image covering `rect` (pixels of its space) at
    /// `pixel_scale` pixels per texel, usable as a render attachment and storage
    /// texture. Its contents are undefined (clear
    /// or overwrite all of `rect`). The texture comes from the renderer's pool
    /// and returns there when every clone of the image is dropped.
    pub fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        self.allocate_format(rect, pixel_scale, self.format)
    }

    /// A pooled intermediate with the attachment format required by the effect.
    pub fn allocate_format(
        &mut self,
        rect: IRect,
        pixel_scale: f64,
        format: wgpu::TextureFormat,
    ) -> Result<GpuImage, RenderError> {
        self.allocator.allocate(rect, pixel_scale, format)
    }
}

impl fmt::Debug for EncodeContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodeContext")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// Hands out pooled working images (implemented by the renderer).
pub(crate) trait ImageAllocator {
    fn allocate(
        &mut self,
        rect: IRect,
        pixel_scale: f64,
        format: wgpu::TextureFormat,
    ) -> Result<GpuImage, RenderError>;
}

/// Pixels of a layer (footage, shapes, text, generators).
pub trait LayerSource: Send + Sync {
    /// Layer-space size in pixels: the source covers `[0, w) × [0, h)`.
    fn size(&self) -> [u32; 2];

    /// Layer-space pixel bounds of the content. Defaults to `[0, w) × [0, h)`
    /// from [`size`](Self::size). Vector sources (shape and text layers, 05) are
    /// drawn around the layer origin and report bounds that may start at negative
    /// coordinates; requests and returned images use the same layer pixels.
    fn bounds(&self) -> IRect {
        let [w, h] = self.size();
        IRect::from_size(w, h)
    }

    /// Hash of everything that affects the pixels at `time` (cache key part).
    fn content_hash(&self, time: Time) -> u64;

    /// Frame rate of a time-based source (footage), for frame blending; `None`
    /// for stills and generated content.
    fn frame_rate(&self) -> Option<bae_model::FrameRate> {
        None
    }

    /// Produces an image covering at least `request.roi`. Implementations either
    /// return a texture they own (decoded footage: zero copy) or draw into an
    /// image from [`EncodeContext::allocate`].
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        request: &SourceRequest,
    ) -> Result<GpuImage, RenderError>;
}

impl fmt::Debug for dyn LayerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayerSource")
            .field("size", &self.size())
            .finish_non_exhaustive()
    }
}

/// Creates sources for [`CustomContent`] of one provider ID.
pub trait SourceProvider: Send + Sync {
    /// Provider ID matched against [`CustomContent::provider`].
    fn id(&self) -> &str;

    /// The source for `content`. Providers may cache by `content.hash`.
    fn source(&self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError>;
}

/// Source providers by ID (built-in and plugin providers alike).
#[derive(Default)]
pub struct SourceProviders {
    providers: FxHashMap<String, Arc<dyn SourceProvider>>,
}

impl SourceProviders {
    /// No providers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) a provider; returns the previous one.
    pub fn register(
        &mut self,
        provider: Arc<dyn SourceProvider>,
    ) -> Option<Arc<dyn SourceProvider>> {
        self.providers.insert(provider.id().to_owned(), provider)
    }

    /// Removes a provider.
    pub fn unregister(&mut self, id: &str) -> Option<Arc<dyn SourceProvider>> {
        self.providers.remove(id)
    }

    /// The provider for `id`.
    pub fn get(&self, id: &str) -> Option<&Arc<dyn SourceProvider>> {
        self.providers.get(id)
    }

    /// Resolves custom content.
    pub fn resolve(&self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        self.get(&content.provider)
            .ok_or_else(|| RenderError::MissingProvider(content.provider.clone()))?
            .source(content)
    }
}

impl fmt::Debug for SourceProviders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.providers.keys()).finish()
    }
}
