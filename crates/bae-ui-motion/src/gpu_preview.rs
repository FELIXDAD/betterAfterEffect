//! The viewer's GPU preview: 04's compositor (`bae_render::Renderer`) renders the
//! evaluated comp, converts it to an 8-bit texture on the GPU and publishes it in
//! the shell's `ViewportTextureRegistry`; the viewer draws it with
//! `egui::TextureId::User`. Pixels never leave the GPU.
//!
//! The comp is converted from the viewer's own `EvaluatedComp` (with live drag
//! overrides applied), so dragging previews through the real compositor too.

use std::sync::Arc;

use bae_anim::EvaluatedComp;
use bae_app::{Registry, Resources};
use bae_gpu::GpuContext;
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::wgpu;
use bae_model::{Stage, Time};
use bae_render::{
    AnimEvaluator, IRect, OutputAlpha, RenderPurpose, RenderRequest, Renderer, RendererConfig,
    Resolution, SourceProvider,
};

/// Layer source providers registered by other modules (05's shape layers, plugin
/// generators), shared with the viewer as an app resource.
#[derive(Clone)]
pub struct PreviewSources(pub Arc<Registry<dyn SourceProvider>>);

impl std::fmt::Debug for PreviewSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewSources").finish_non_exhaustive()
    }
}

/// Effect implementations registered by other modules (07's built-ins, plugin
/// effects), shared with the viewer and the render queue as an app resource.
#[derive(Clone)]
pub struct PreviewEffects(pub Arc<Registry<dyn bae_render::Effect>>);

impl std::fmt::Debug for PreviewEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewEffects").finish_non_exhaustive()
    }
}

/// What the viewer draws: an egui user texture covering `rect` (comp pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PreviewImage {
    pub handle: u64,
    pub rect: IRect,
}

pub(crate) struct GpuPreview {
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    renderer: Renderer,
    anim: AnimEvaluator,
    sources_gen: Option<u64>,
    /// `(content hash, time, resolution)` of the published frame.
    last: Option<(u64, Time, u32)>,
    rect: Option<IRect>,
    /// Text layers have a source provider (otherwise the viewer draws them).
    pub has_text: bool,
}

impl GpuPreview {
    /// A preview on the app's GPU context, if the app has one (the headless test
    /// harness and agents do not).
    pub fn new(resources: &Resources) -> Option<Self> {
        let gpu = resources.get::<Arc<GpuContext>>()?.clone();
        let registry = resources.get::<Arc<ViewportTextureRegistry>>()?.clone();
        Some(Self::with(gpu, registry))
    }

    pub fn with(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        let id = registry.create("motion composition viewer");
        let renderer = Renderer::with_config(
            gpu.clone(),
            RendererConfig {
                frame_cache_bytes: 512 << 20,
                pool_idle_bytes: 128 << 20,
            },
        );
        GpuPreview {
            gpu,
            registry,
            id,
            renderer,
            anim: AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview),
            sources_gen: None,
            last: None,
            rect: None,
            has_text: false,
        }
    }

    /// Whether this preview still uses the app's current GPU context (the shell
    /// recreates it after a device loss).
    pub fn is_current(&self, resources: &Resources) -> bool {
        resources
            .get::<Arc<GpuContext>>()
            .is_some_and(|g| Arc::ptr_eq(g, &self.gpu))
            && !self.gpu.is_lost()
    }

    fn sync_sources(&mut self, resources: &Resources) {
        let Some(PreviewSources(registry)) = resources.get::<PreviewSources>() else {
            return;
        };
        let generation = registry.generation();
        if self.sources_gen == Some(generation) {
            return;
        }
        self.sources_gen = Some(generation);
        let providers = self.renderer.providers_mut();
        for entry in registry.snapshot().iter() {
            providers.register(entry.item().clone());
        }
        self.has_text = providers.get(bae_render::anim::TEXT_PROVIDER).is_some();
        self.last = None;
    }

    fn sync_effects(&mut self, resources: &Resources) {
        if let Some(PreviewEffects(registry)) = resources.get::<PreviewEffects>()
            && self.renderer.sync_effects(registry)
        {
            self.last = None;
        }
    }

    /// Render `comp` (if it changed) and return the texture to draw. `None` until
    /// the shell has registered the texture with egui (the frame after the first
    /// publish) or when rendering failed.
    pub fn frame(
        &mut self,
        resources: &Resources,
        stage: &Stage,
        comp: &EvaluatedComp,
        resolution: u32,
    ) -> Option<PreviewImage> {
        self.sync_sources(resources);
        self.sync_effects(resources);
        let rc = match self.anim.convert(stage, comp, 0) {
            Ok(rc) => rc,
            Err(e) => {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
        };
        let key = (rc.content_hash(), rc.time, resolution);
        if self.last != Some(key) {
            let request = RenderRequest {
                resolution: match resolution {
                    2 => Resolution::Half,
                    3 => Resolution::Third,
                    4 => Resolution::Quarter,
                    _ => Resolution::Full,
                },
                purpose: RenderPurpose::Preview,
                ..RenderRequest::full()
            };
            let frame = match self.renderer.render_frame(&rc, &request) {
                Ok(f) => f,
                Err(e) => {
                    tracing_warn(&format!("preview: {e}"));
                    return None;
                }
            };
            let texture = match self
                .renderer
                .output_texture(&frame, OutputAlpha::Premultiplied)
            {
                Ok(t) => t,
                Err(e) => {
                    tracing_warn(&format!("preview: {e}"));
                    return None;
                }
            };
            // Display-encoded 8-bit values, sampled as they are (egui blends in
            // gamma space, see docs/spec/app-shell.md §5).
            if let Err(e) = self
                .registry
                .publish(self.id, &texture, wgpu::FilterMode::Linear)
            {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
            self.last = Some(key);
            self.rect = Some(frame.image.rect);
        }
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect?,
        })
    }

    #[cfg(test)]
    pub fn renderer(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    #[cfg(test)]
    pub fn viewport(&self) -> ViewportTextureId {
        self.id
    }
}

impl Drop for GpuPreview {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

fn tracing_warn(message: &str) {
    // Once per distinct message: the viewer calls this every frame.
    thread_local! {
        static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    LAST.with(|l| {
        if *l.borrow() != message {
            tracing::warn!("{message}");
            *l.borrow_mut() = message.to_owned();
        }
    });
}
