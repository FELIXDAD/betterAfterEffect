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
use bae_model::{PrimPath, Stage, Time};
use bae_render::{
    AnimEvaluator, IRect, OutputAlpha, OutputTarget, RenderPurpose, RenderRequest, Renderer,
    RendererConfig, Resolution, SourceProvider,
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

pub(super) struct RenderEngine {
    pub(super) renderer: Renderer,
    pub(super) anim: AnimEvaluator,
    sources_gen: Option<u64>,
}

impl RenderEngine {
    pub(super) fn new(gpu: Arc<GpuContext>) -> Self {
        Self {
            renderer: Renderer::with_config(
                gpu,
                RendererConfig {
                    frame_cache_bytes: 0,
                    pool_idle_bytes: 128 << 20,
                },
            ),
            sources_gen: None,
            anim: AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview),
        }
    }

    pub(super) fn sync(
        &mut self,
        sources: Option<&PreviewSources>,
        effects: Option<&PreviewEffects>,
    ) -> bool {
        let mut changed = false;
        if let Some(PreviewSources(registry)) = sources {
            let generation = registry.generation();
            if self.sources_gen != Some(generation) {
                for entry in registry.snapshot().iter() {
                    self.renderer.providers_mut().register(entry.item().clone());
                }
                self.sources_gen = Some(generation);
                changed = true;
            }
        }
        let effects_changed = effects.is_some_and(|e| self.renderer.sync_effects(&e.0));
        changed || effects_changed
    }

    pub(super) fn render(
        &mut self,
        stage: &Stage,
        rc: &bae_render::RenderComp,
        resolution: u32,
        available: Option<OutputTarget>,
    ) -> Result<(OutputTarget, IRect, bool), bae_render::RenderError> {
        let request = RenderRequest {
            resolution: Resolution::Custom(resolution),
            purpose: RenderPurpose::Preview,
            ..RenderRequest::full()
        };
        let mut frames = SampledFrames {
            sampled: false,
            inner: bae_render::eval::EvaluatorFrames {
                stage,
                evaluator: &mut self.anim,
            },
        };
        let frame = self
            .renderer
            .render_frame_with(rc, &request, Some(&mut frames))?;
        let texture = self
            .renderer
            .output_to(&frame, OutputAlpha::Premultiplied, available)?;
        Ok((texture, frame.image.rect, frames.sampled))
    }
}

struct SampledFrames<'a> {
    inner: bae_render::eval::EvaluatorFrames<'a>,
    sampled: bool,
}

impl bae_render::graph::CompFrames for SampledFrames<'_> {
    fn comp_at(
        &mut self,
        path: &bae_model::PrimPath,
        t: Time,
    ) -> Result<Option<bae_render::RenderComp>, bae_render::RenderError> {
        self.sampled = true;
        self.inner.comp_at(path, t)
    }
}

pub(crate) struct GpuPreview {
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    engine: Option<RenderEngine>,
    pub(super) playback: super::playback::PlaybackPreview,
    target: Option<OutputTarget>,
    last: Option<(u64, PrimPath, Time, u32)>,
    rect: Option<IRect>,
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
        let playback = super::playback::PlaybackPreview::new(gpu.clone(), registry.clone());
        GpuPreview {
            gpu,
            registry,
            id,
            engine: None,
            playback,
            target: None,
            last: None,
            rect: None,
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

    /// Render `comp` (if it changed) and return the texture to draw. `None` until
    /// the shell has registered the texture with egui (the frame after the first
    /// publish) or when rendering failed.
    pub fn frame(
        &mut self,
        resources: &Resources,
        stage: &Stage,
        comp: &EvaluatedComp,
        resolution: u32,
        live_edit: bool,
    ) -> Option<PreviewImage> {
        let engine = self
            .engine
            .get_or_insert_with(|| RenderEngine::new(self.gpu.clone()));
        if engine.sync(
            resources.get::<PreviewSources>(),
            resources.get::<PreviewEffects>(),
        ) {
            self.last = None;
        }
        let key = (stage.revision(), comp.path.clone(), comp.time, resolution);
        if live_edit || self.last.as_ref() != Some(&key) {
            self.last = None;
            let rc = engine
                .anim
                .convert(stage, comp, 0)
                .map_err(|e| tracing_warn(&format!("preview: {e}")))
                .ok()?;
            let previous_size = self.target.as_ref().map(OutputTarget::size);
            let (target, rect, _) = engine
                .render(stage, &rc, resolution, self.target.take())
                .map_err(|e| tracing_warn(&format!("preview: {e}")))
                .ok()?;
            if previous_size != Some(target.size()) {
                self.registry
                    .publish_view(
                        self.id,
                        target.view().clone(),
                        target.size(),
                        wgpu::TextureFormat::Rgba8Unorm,
                        wgpu::FilterMode::Linear,
                    )
                    .map_err(|e| tracing_warn(&format!("preview: {e}")))
                    .ok()?;
            }
            self.target = Some(target);
            self.last = (!live_edit).then_some(key);
            self.rect = Some(rect);
        }
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect?,
        })
    }

    #[cfg(test)]
    pub fn renderer(&mut self) -> &mut Renderer {
        &mut self
            .engine
            .get_or_insert_with(|| RenderEngine::new(self.gpu.clone()))
            .renderer
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
