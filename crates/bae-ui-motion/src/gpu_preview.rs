//! The viewer's GPU preview: 04's compositor (`bae_render::Renderer`) renders the
//! evaluated comp, converts it to an 8-bit texture on the GPU and publishes it in
//! the shell's `ViewportTextureRegistry`; the viewer draws it with
//! `egui::TextureId::User`. Pixels never leave the GPU.
//!
//! The comp is converted from the viewer's own `EvaluatedComp` (with live drag
//! overrides applied), so dragging previews through the real compositor too.

use std::sync::Arc;

use crate::common::StageChanges;
use bae_anim::EvaluatedComp;
use bae_app::{Registry, Resources};
use bae_gpu::GpuContext;
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::wgpu;
use bae_model::{Stage, Time};
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

/// Picker data belongs to the same composition and time as the displayed preview.
#[derive(Debug)]
pub(crate) struct ChannelFrame {
    pub comp: bae_model::PrimPath,
    pub time: Time,
    pub layers: std::collections::HashMap<bae_model::PrimPath, Arc<bae_gpu::channels::ChannelInfo>>,
}
impl ChannelFrame {
    fn from_comp(renderer: &Renderer, rc: &bae_render::RenderComp) -> Option<Arc<Self>> {
        let mut layers = std::collections::HashMap::new();
        for layer in &rc.layers {
            if layer.effects.iter().any(|e| {
                matches!(
                    e.effect.as_str(),
                    "Cryptomatte" | "EXtractoR" | "IDentifier"
                )
            }) && let bae_render::LayerContent::Footage(f) = &layer.content
                && let Some(info) = renderer.footage_channels(f)
            {
                layers.insert(layer.path.clone(), info);
            }
        }
        (!layers.is_empty()).then(|| {
            Arc::new(Self {
                comp: rc.path.clone(),
                time: rc.time,
                layers,
            })
        })
    }
}
pub(super) struct RenderEngine {
    pub(super) channels: Option<Arc<ChannelFrame>>,
    pub(super) renderer: Renderer,
    pub(super) anim: AnimEvaluator,
    /// The comp and per-layer render times of the last frame rendered (not
    /// from the cache) while [`Renderer::set_layer_times`] was on.
    pub(super) layer_times: Option<(
        bae_model::PrimPath,
        Vec<(bae_model::PrimPath, std::time::Duration)>,
    )>,
}

impl RenderEngine {
    pub(super) fn new(gpu: Arc<GpuContext>, cache: u64) -> Self {
        Self {
            channels: None,
            renderer: Renderer::with_config(
                gpu,
                RendererConfig {
                    frame_cache_bytes: cache,
                    pool_idle_bytes: 128 << 20,
                },
            ),
            anim: AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview),
            layer_times: None,
        }
    }

    /// Draws the layers `samples` asks for (`sampleImage`) and starts reading
    /// them back without waiting ([`Renderer::poll_samples`] stores them).
    /// Samples are read at full resolution, whatever the preview's.
    pub(super) fn read_samples(
        &mut self,
        stage: &Stage,
        rc: &bae_render::RenderComp,
        samples: &bae_anim::ImageSamples,
    ) -> Result<(), bae_render::RenderError> {
        let request = RenderRequest {
            purpose: RenderPurpose::Preview,
            ..RenderRequest::full()
        };
        let mut frames = bae_render::eval::EvaluatorFrames {
            stage,
            evaluator: &mut self.anim,
        };
        self.renderer
            .read_samples(samples, rc, &request, Some(&mut frames))
    }

    /// The frame converted to an 8-bit target (`reuse` when it has the
    /// frame's size) and its floating-point image, `None` when the frame is incomplete
    /// (something still warming up: not shown, and `reuse` comes back
    /// untouched).
    pub(super) fn render(
        &mut self,
        stage: &Stage,
        rc: &bae_render::RenderComp,
        resolution: u32,
        display: bae_render::Display,
        roi: Option<IRect>,
        reuse: Option<OutputTarget>,
    ) -> Result<(OutputTarget, Option<bae_render::GpuImage>, bool), bae_render::RenderError> {
        let request = RenderRequest {
            resolution: match resolution {
                0 | 1 => Resolution::Full,
                2 => Resolution::Half,
                3 => Resolution::Third,
                4 => Resolution::Quarter,
                n => Resolution::Custom(n),
            },
            purpose: RenderPurpose::Preview,
            roi,
            ..RenderRequest::full()
        };
        let mut frames = SampledFrames {
            sampled: false,
            inner: bae_render::eval::EvaluatorFrames {
                stage,
                evaluator: &mut self.anim,
            },
        };
        let (frame, target) = if display == bae_render::Display::default() {
            // The frame and its 8-bit conversion in one GPU submission.
            self.renderer.render_frame_output(
                rc,
                &request,
                Some(&mut frames),
                OutputAlpha::Premultiplied,
                reuse,
            )?
        } else {
            // Show Channel / Exposure: the conversion applies them.
            let frame = self
                .renderer
                .render_frame_with(rc, &request, Some(&mut frames))?;
            let target = self.renderer.output_texture_display(
                &frame,
                OutputAlpha::Premultiplied,
                display,
                reuse,
            )?;
            (frame, target)
        };
        self.layer_times = (!frame.from_cache).then(|| (frame.comp.clone(), frame.layer_times));
        self.channels = ChannelFrame::from_comp(&self.renderer, rc);
        let source = (!frame.incomplete).then_some(frame.image);
        Ok((target, source, frames.sampled))
    }
}

/// Other frames the render sampled (time effects): such frames depend on
/// more than their own time.
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
    engine: RenderEngine,
    pub(super) playback: super::playback::PlaybackPreview,
    changes: StageChanges,
    sources_gen: Option<u64>,
    solid_reload: u64,
    /// Edit ▸ Purge generation last applied (`Session::purge`).
    purged: u64,
    /// `(content hash, time, resolution)` of the published frame.
    last: Option<(u64, Time, u32)>,
    rect: Option<IRect>,
    /// The published frame (the Info panel reads pixels from it) and the last
    /// pixel read: (texel, value).
    target: Option<OutputTarget>,
    direct_source: Option<bae_render::GpuImage>,
    sampled: Option<(PixelKey, [u8; 4])>,
    pending_pixel: Option<(PixelKey, bae_gpu::transfer::PendingReadback)>,
    /// How the viewer shows the frame (Show Channel, Exposure).
    pub display: bae_render::Display,
    /// Region of interest (comp pixels): only it is rendered.
    pub roi: Option<IRect>,
    /// The display and region of the published frame.
    last_view: Option<(bae_render::Display, Option<IRect>)>,
    /// Snapshots 1–4 (AE's Take Snapshot): a published frame.
    snapshots: [Option<(wgpu::Texture, IRect)>; 4],
    /// The registry shows `target`'s view (not a snapshot).
    target_shown: bool,
    /// A frame of the background renderer or its cache the viewer shows
    /// instead (the Info panel and snapshots read it).
    cached: Option<(wgpu::Texture, IRect)>,
    /// The newest frame was incomplete (something still warming up): the
    /// viewer keeps showing the last complete frame with a "Preparing…" hint
    /// until it is redrawn complete.
    pub preparing: bool,
    /// Renders the newest dragged value in the background (#593).
    live: crate::live_preview::LivePreview,
    /// The project version live frames render with.
    live_scene: Option<Arc<crate::live_preview::LiveScene>>,
    /// The key and view of the last live request.
    live_sent: Option<LiveKey>,
    /// The live target the registry shows.
    live_texture: Option<wgpu::Texture>,
    live_generation: u64,
    /// Layer pixels for `sampleImage`, shared with the evaluator that evaluates
    /// the comps this preview shows.
    samples: Arc<bae_anim::ImageSamples>,
    /// The last frame held the previous image back for missing samples.
    waiting: bool,
}

type PixelKey = (u32, u32, u64);

type LiveKey = ((u64, Time, u32), (bae_render::Display, Option<IRect>));

impl GpuPreview {
    /// A preview on the app's GPU context, if the app has one (the headless test
    /// harness and agents do not).
    pub fn new(resources: &Resources, ui: &egui::Context) -> Option<Self> {
        let gpu = resources.get::<Arc<GpuContext>>()?.clone();
        let registry = resources.get::<Arc<ViewportTextureRegistry>>()?.clone();
        let mut preview = Self::with(gpu, registry);
        let ui = ui.clone();
        preview
            .engine
            .anim
            .prepare_preview_audio(move || ui.request_repaint());
        Some(preview)
    }

    pub fn with(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        let id = registry.create("motion composition viewer");
        let mut engine = RenderEngine::new(gpu.clone(), 512 << 20);
        let samples = Arc::new(bae_anim::ImageSamples::new());
        bae_render::CompEvaluator::set_image_samples(&mut engine.anim, Some(samples.clone()));
        let playback = super::playback::PlaybackPreview::new(gpu.clone(), registry.clone());
        let live = crate::live_preview::LivePreview::new(gpu.clone());
        GpuPreview {
            gpu,
            registry,
            id,
            engine,
            playback,
            changes: StageChanges::default(),
            sources_gen: None,
            solid_reload: 0,
            purged: 0,
            last: None,
            rect: None,
            target: None,
            direct_source: None,
            sampled: None,
            pending_pixel: None,
            display: bae_render::Display::default(),
            roi: None,
            last_view: None,
            snapshots: Default::default(),
            target_shown: false,
            cached: None,
            preparing: false,
            live,
            live_scene: None,
            live_sent: None,
            live_texture: None,
            live_generation: 0,
            samples,
            waiting: false,
        }
    }

    /// The `sampleImage` cache: give it to the evaluator of the comps passed to
    /// [`GpuPreview::frame`] ([`bae_anim::Evaluator::set_image_samples`]).
    pub fn samples(&self) -> &Arc<bae_anim::ImageSamples> {
        &self.samples
    }

    /// The last [`GpuPreview::frame`] kept the previous image while `sampleImage`
    /// values were read back: repaint (evaluate again) soon.
    pub fn waiting_for_samples(&self) -> bool {
        self.waiting
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
        let providers = self.engine.renderer.providers_mut();
        for entry in registry.snapshot().iter() {
            providers.register(entry.item().clone());
        }
        self.last = None;
    }

    fn sync_effects(&mut self, resources: &Resources) {
        if let Some(PreviewEffects(registry)) = resources.get::<PreviewEffects>()
            && self.engine.renderer.sync_effects(registry)
        {
            self.last = None;
        }
    }

    /// Edit ▸ Purge ▸ Image Cache Memory / All Memory: drop the rendered frames
    /// (and reopen the footage) once per purge `generation`.
    pub(super) fn purge(&mut self, generation: u64) {
        if self.purged == generation {
            return;
        }
        self.engine.renderer.reload_footage();
        self.engine.anim.clear_compiled();
        self.last = None;
        self.purged = generation;
    }

    pub(super) fn reload_solid(&mut self, generation: u64, item: &bae_model::PrimPath) {
        if self.solid_reload == generation {
            return;
        }
        let event = bae_model::StageEvent {
            origin: bae_model::EventOrigin::Local,
            author: None,
            sublayers: vec![],
            changes: vec![bae_model::Change::Resynced { path: item.clone() }],
        };
        self.engine.renderer.invalidate(&event);
        self.engine.anim.clear_compiled();
        self.last = None;
        self.playback.invalidate_source(&event);
        self.solid_reload = generation;
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
        camera: Option<bae_render::Camera>,
        measure_layer_times: bool,
    ) -> Option<PreviewImage> {
        if self.cached.take().is_some() {
            self.sampled = None;
            self.pending_pixel = None;
        }
        self.sync_sources(resources);
        self.sync_effects(resources);
        // A value or keyframe drag in a panel: motion blur samples and effects
        // reading other frames evaluate the uncommitted values too (#593).
        let overrides = resources
            .get::<crate::session::SharedSession>()
            .map(|s| s.lock().live_overrides(stage))
            .unwrap_or_default();
        let live = (!overrides.is_empty()).then(|| overrides.clone());
        self.engine.anim.set_nested_switches(
            crate::prefs::current(resources).bool("composition.switches_affect_nested_comps"),
        );
        self.engine.anim.set_overrides(overrides);
        if self.changes.sync(stage, |event| match event {
            Some(event) => bae_render::CompEvaluator::invalidate(&mut self.engine.anim, event),
            None => self.engine.anim.clear_compiled(),
        }) {
            self.last = None;
        }
        let mut rc = match self.engine.anim.convert(stage, comp, 0) {
            Ok(rc) => rc,
            Err(e) => {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
        };
        if self.engine.anim.audio_preparation_pending() {
            return self.image();
        }
        // A 3D view other than Active Camera (the output never changes).
        if let Some(camera) = camera {
            rc.camera = Some(camera);
            if let Some(blur) = &mut rc.motion_blur {
                let blur = std::sync::Arc::make_mut(blur);
                blur.cameras.fill(Some(camera));
            }
        }
        // sampleImage: the layers expressions sampled are drawn and read back
        // without waiting (during a value drag on the live renderer's thread).
        // A frame evaluated before its samples arrived is not shown: the
        // previous image stays up and the viewer evaluates again.
        if live.is_none()
            && self.samples.has_requests()
            && let Err(e) = self.engine.read_samples(stage, &rc, &self.samples)
        {
            tracing_warn(&format!("preview: sampleImage: {e}"));
        }
        let arrived = self.engine.renderer.poll_samples(&self.samples);
        self.waiting = arrived || self.samples.is_waiting();
        if self.waiting && live.is_none() && self.rect.is_some() {
            return Some(PreviewImage {
                handle: self.registry.display_handle(self.id)?,
                rect: self.rect?,
            });
        }
        let key = (self.engine.renderer.content_hash(&rc), rc.time, resolution);
        let view = (self.display, self.roi);
        // A value drag: the newest value renders in the background.
        if let Some(overrides) = live {
            return self.live_frame(
                resources,
                stage,
                rc,
                (key, view),
                resolution,
                measure_layer_times,
                overrides,
            );
        }
        self.live_sent = None;
        if self.last != Some(key) || self.last_view != Some(view) {
            self.last_view = Some(view);
            self.engine.renderer.set_layer_times(measure_layer_times);
            let old_size = self.target.as_ref().map(OutputTarget::size);
            let rendered = match self.engine.render(
                stage,
                &rc,
                resolution,
                self.display,
                self.roi,
                self.target.take(),
            ) {
                Ok(rendered) => rendered,
                Err(e) => {
                    tracing_warn(&format!("preview: {e}"));
                    return None;
                }
            };
            if let Some((comp, times)) = self.engine.layer_times.take()
                && measure_layer_times
                && let Some(session) = resources.get::<crate::session::SharedSession>()
            {
                let mut s = session.lock();
                s.layer_times.retain(|layer, _| !comp.is_prefix_of(layer));
                s.layer_times.extend(times);
            }
            // Like AE, an incomplete frame (an effect pipeline or the text
            // renderer still warming up) is never shown: the last complete
            // frame stays, and the frame is rendered again on the redraw the
            // warm-up requests when it is ready.
            let (target, source, _) = rendered;
            self.preparing = source.is_none();
            let Some(source) = source else {
                // The published target (if any) still shows the last complete
                // frame; a new one holds the incomplete frame and is dropped.
                if old_size.is_some() {
                    self.target = Some(target);
                }
                return Some(PreviewImage {
                    handle: self.registry.display_handle(self.id)?,
                    rect: self.rect?,
                });
            };
            // Display-encoded 8-bit values, sampled as they are (egui blends in
            // gamma space, see docs/spec/app-shell.md §5).
            // Egui keeps the same view until the render dimensions change.
            if (old_size != Some(target.size()) || !self.target_shown)
                && let Err(e) = self.registry.publish_view(
                    self.id,
                    target.view().clone(),
                    target.size(),
                    target.view().texture().format(),
                    wgpu::FilterMode::Linear,
                )
            {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
            if let Some(session) = resources.get::<crate::session::SharedSession>() {
                session.lock().channel_frame = self.engine.channels.clone();
            }
            self.last = Some(key);
            self.rect = Some(source.rect);
            self.direct_source = Some(source);
            self.target = Some(target);
            self.target_shown = true;
            self.live_texture = None;
            self.sampled = None;
            self.pending_pixel = None;
        }
        self.image()
    }

    /// The frame to draw; back from a snapshot, the frame is shown again.
    fn image(&mut self) -> Option<PreviewImage> {
        if !self.target_shown
            && let Some(t) = &self.target
            && self
                .registry
                .publish_view(
                    self.id,
                    t.view().clone(),
                    t.size(),
                    t.view().texture().format(),
                    wgpu::FilterMode::Linear,
                )
                .is_ok()
        {
            self.target_shown = true;
            self.live_texture = None;
        }
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect?,
        })
    }

    /// The frame the viewer shows and a key that changes with it (Lumetri
    /// Scopes): a cached frame of the background renderer, else the rendered
    /// one.
    pub fn output(&self) -> Option<(wgpu::Texture, u64)> {
        use std::hash::{Hash, Hasher};
        if let Some((t, _)) = &self.cached {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            t.hash(&mut h);
            return Some((t.clone(), h.finish()));
        }
        if let Some(texture) = &self.live_texture {
            return Some((texture.clone(), self.live_generation));
        }
        let (hash, time, resolution) = self.last?;
        let key = hash ^ (time.flicks() as u64).rotate_left(17) ^ u64::from(resolution);
        Some((self.target.as_ref()?.view().texture().clone(), key))
    }

    /// Keep the pool lease alive while Lumetri samples the working buffer.
    pub(super) fn scope_source(&self) -> Option<bae_render::GpuImage> {
        if self.cached.is_some() {
            self.playback.scope_source()
        } else {
            self.direct_source.clone()
        }
    }

    pub(super) fn snapshot_texture(&self, slot: usize) -> Option<wgpu::Texture> {
        self.snapshots.get(slot)?.as_ref().map(|s| s.0.clone())
    }

    /// The viewer shows `texture` (a frame of the background renderer or
    /// its cache): the Info panel and snapshots read it until the next
    /// [`Self::frame`].
    pub fn show_cached(&mut self, texture: wgpu::Texture, rect: IRect) {
        if self.cached.as_ref().map(|c| &c.0) != Some(&texture) {
            self.direct_source = None;
            self.cached = Some((texture, rect));
            self.sampled = None;
            self.pending_pixel = None;
        }
    }

    /// Take Snapshot: keep the frame shown now in `slot` (0–3).
    pub fn take_snapshot(&mut self, slot: usize) -> bool {
        let Some(s) = self.snapshots.get_mut(slot) else {
            return false;
        };
        // A cached frame's texture is never rendered into again.
        if let Some(cached) = &self.cached {
            *s = Some(cached.clone());
            return true;
        }
        let Some(r) = self.rect else {
            return false;
        };
        // The snapshot keeps the frame's texture: the next frame renders into
        // a new one instead of reusing it.
        let Some(target) = self.target.take() else {
            return false;
        };
        *s = Some((target.view().texture().clone(), r));
        self.last = None;
        true
    }

    pub fn purge_snapshot(&mut self, slot: usize) {
        if let Some(s) = self.snapshots.get_mut(slot) {
            *s = None;
        }
    }

    /// Show Snapshot: publish snapshot `slot` instead of the frame (the next
    /// [`GpuPreview::frame`] shows the frame again).
    /// The project version live frames render with (kept for the drag).
    fn live_scene(
        &mut self,
        resources: &Resources,
        stage: &Stage,
    ) -> Arc<crate::live_preview::LiveScene> {
        let sources = resources.get::<PreviewSources>();
        let effects = resources.get::<PreviewEffects>();
        let scene_key = (
            stage.revision(),
            sources.map(|s| s.0.generation()),
            effects.map(|e| e.0.generation()),
        );
        let nested_switches =
            crate::prefs::current(resources).bool("composition.switches_affect_nested_comps");
        match &self.live_scene {
            Some(scene) if scene.key == scene_key && scene.nested_switches == nested_switches => {
                scene.clone()
            }
            _ => {
                let scene = Arc::new(crate::live_preview::LiveScene {
                    key: scene_key,
                    nested_switches,
                    snapshot: crate::preview_snapshot::PreviewSnapshot::take(stage),
                    sources: sources.cloned(),
                    effects: effects.cloned(),
                });
                self.live_scene = Some(scene.clone());
                scene
            }
        }
    }

    /// [`GpuPreview::frame`] during a value drag: submits the newest value to
    /// the background renderer (replacing a request that has not started)
    /// and shows the newest finished frame, never waiting for one.
    #[allow(clippy::too_many_arguments)]
    fn live_frame(
        &mut self,
        resources: &Resources,
        stage: &Stage,
        rc: bae_render::RenderComp,
        sent: LiveKey,
        resolution: u32,
        measure_layer_times: bool,
        overrides: Vec<bae_anim::AttrOverride>,
    ) -> Option<PreviewImage> {
        use crate::live_preview::LiveJob;
        // The frame is rendered again here once the drag ends.
        self.last = None;
        // Samples this value needs are read on the live thread first; the
        // frame itself goes once they are all in (the viewer evaluates again).
        let reading = self.samples.has_requests();
        if reading || self.waiting {
            if reading {
                let scene = self.live_scene(resources, stage);
                self.live.submit(LiveJob {
                    scene,
                    rc,
                    overrides,
                    resolution,
                    display: self.display,
                    roi: self.roi,
                    measure: false,
                    samples: Some(self.samples.clone()),
                    render: false,
                });
            }
            return Some(PreviewImage {
                handle: self.registry.display_handle(self.id)?,
                rect: self.rect?,
            });
        }
        if self.live_sent != Some(sent) {
            let scene = self.live_scene(resources, stage);
            self.live.submit(LiveJob {
                scene,
                rc,
                overrides,
                resolution,
                display: self.display,
                roi: self.roi,
                measure: measure_layer_times,
                samples: Some(self.samples.clone()),
                render: true,
            });
            self.live_sent = Some(sent);
        }
        if let Some(frame) = self.live.take() {
            self.preparing = frame.source.is_none();
            if let Some(source) = frame.source {
                if self.live_texture.as_ref() != Some(frame.view.texture()) {
                    if let Err(e) = self.registry.publish_view(
                        self.id,
                        frame.view.clone(),
                        frame.size,
                        frame.view.texture().format(),
                        wgpu::FilterMode::Linear,
                    ) {
                        tracing_warn(&format!("preview: {e}"));
                        return None;
                    }
                    self.live_texture = Some(frame.view.texture().clone());
                    self.target_shown = false;
                }
                self.rect = Some(source.rect);
                self.direct_source = Some(source);
                self.live_generation = self.live_generation.wrapping_add(1);
                self.sampled = None;
                self.pending_pixel = None;
            }
            if measure_layer_times
                && let Some((comp, times)) = frame.layer_times
                && let Some(session) = resources.get::<crate::session::SharedSession>()
            {
                let mut s = session.lock();
                s.layer_times.retain(|layer, _| !comp.is_prefix_of(layer));
                s.layer_times.extend(times);
            }
        }
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect?,
        })
    }

    pub fn show_snapshot(&mut self, slot: usize) -> Option<PreviewImage> {
        let (t, rect) = self.snapshots.get(slot)?.clone()?;
        self.registry
            .publish(self.id, &t, wgpu::FilterMode::Linear)
            .ok()?;
        self.target_shown = false;
        self.live_texture = None;
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect,
        })
    }

    /// Straight (unpremultiplied) 8-bit RGBA of the published frame at comp
    /// point `p`, transparent outside the image. Starts at most one readback
    /// at a time and requests a redraw when the requested texel is pending;
    /// until it arrives, the previous reading.
    pub fn pixel(&mut self, p: [f64; 2], repaint: &egui::Context) -> Option<[u8; 4]> {
        let (texture, rect) = match &self.cached {
            Some((texture, rect)) => (texture, *rect),
            None => (self.target.as_ref()?.view().texture(), self.rect?),
        };
        let (w, h) = (texture.width(), texture.height());
        let (rw, rh) = (f64::from(rect.x1 - rect.x0), f64::from(rect.y1 - rect.y0));
        if rw <= 0.0 || rh <= 0.0 {
            return None;
        }
        let u = (p[0] - f64::from(rect.x0)) / rw;
        let v = (p[1] - f64::from(rect.y0)) / rh;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return Some([0; 4]);
        }
        let texel = (
            ((u * f64::from(w)) as u32).min(w - 1),
            ((v * f64::from(h)) as u32).min(h - 1),
            self.last.map_or(0, |k| k.0 ^ u64::from(k.2)),
        );
        if let Some((key, pending)) = &self.pending_pixel {
            self.gpu.poll();
            if let Some(result) = pending.try_finish() {
                if let Ok(image) = result {
                    let px = image.pixel(0, 0);
                    let a = px[3];
                    let straight = |c: u8| {
                        (u32::from(c) * 255 + u32::from(a) / 2)
                            .checked_div(u32::from(a))
                            .unwrap_or(0)
                            .min(255) as u8
                    };
                    self.sampled =
                        Some((*key, [straight(px[0]), straight(px[1]), straight(px[2]), a]));
                }
                self.pending_pixel = None;
            }
        }
        if let Some((t, c)) = self.sampled
            && t == texel
        {
            return Some(c);
        }
        repaint.request_repaint_after(std::time::Duration::from_millis(8));
        // While the new texel is read back, the last reading stands in (the
        // Info panel and the pen's mask colour follow the moving pointer).
        let last = self.sampled.map(|(_, c)| c);
        if self.pending_pixel.is_some() {
            return last;
        }
        let region = bae_gpu::transfer::TextureRegion {
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: texel.0,
                y: texel.1,
                z: 0,
            },
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        };
        let mut encoder =
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("bae-ui-motion.info-pixel"),
                });
        let readback =
            bae_gpu::transfer::TextureReadback::record(&self.gpu, &mut encoder, texture, region)
                .ok()?;
        self.gpu.queue().submit([encoder.finish()]);
        self.pending_pixel = Some((texel, readback.map()));
        last
    }

    #[cfg(test)]
    pub fn renderer(&mut self) -> &mut Renderer {
        &mut self.engine.renderer
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
