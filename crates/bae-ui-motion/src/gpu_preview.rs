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

pub(super) struct RenderEngine {
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
    /// frame's size) and the frame's rect, `None` when the frame is incomplete
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
    ) -> Result<(OutputTarget, Option<IRect>), bae_render::RenderError> {
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
        let mut frames = bae_render::eval::EvaluatorFrames {
            stage,
            evaluator: &mut self.anim,
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
        Ok((target, (!frame.incomplete).then_some(frame.image.rect)))
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
    /// `(content hash, time, resolution)` of the published frame.
    last: Option<(u64, Time, u32)>,
    rect: Option<IRect>,
    /// The published frame (the Info panel reads pixels from it) and the last
    /// pixel read: (texel, value).
    target: Option<OutputTarget>,
    sampled: Option<((u32, u32, u64), [u8; 4])>,
    /// Text layers have a source provider (otherwise the viewer draws them).
    pub has_text: bool,
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
    /// The newest frame was incomplete (something still warming up): the
    /// viewer keeps showing the last complete frame with a "Preparing…" hint
    /// until it is redrawn complete.
    pub preparing: bool,
    /// Layer pixels for `sampleImage`, shared with the evaluator that evaluates
    /// the comps this preview shows.
    samples: Arc<bae_anim::ImageSamples>,
    /// The last frame held the previous image back for missing samples.
    waiting: bool,
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
        let mut engine = RenderEngine::new(gpu.clone(), 512 << 20);
        let samples = Arc::new(bae_anim::ImageSamples::new());
        bae_render::CompEvaluator::set_image_samples(&mut engine.anim, Some(samples.clone()));
        let playback = super::playback::PlaybackPreview::new(gpu.clone(), registry.clone());
        GpuPreview {
            gpu,
            registry,
            id,
            engine,
            playback,
            changes: StageChanges::default(),
            sources_gen: None,
            last: None,
            rect: None,
            target: None,
            sampled: None,
            has_text: false,
            display: bae_render::Display::default(),
            roi: None,
            last_view: None,
            snapshots: Default::default(),
            target_shown: false,
            preparing: false,
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
        self.has_text = providers.get(bae_render::anim::TEXT_PROVIDER).is_some();
        self.last = None;
    }

    fn sync_effects(&mut self, resources: &Resources) {
        if let Some(PreviewEffects(registry)) = resources.get::<PreviewEffects>()
            && self.engine.renderer.sync_effects(registry)
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
        camera: Option<bae_render::Camera>,
        measure_layer_times: bool,
    ) -> Option<PreviewImage> {
        self.sync_sources(resources);
        self.sync_effects(resources);
        if self.changes.changed(resources) {
            self.engine.anim.clear_compiled();
            self.last = None;
        }
        let mut rc = match self.engine.anim.convert(stage, comp, 0) {
            Ok(rc) => rc,
            Err(e) => {
                tracing_warn(&format!("preview: {e}"));
                return None;
            }
        };
        // A 3D view other than Active Camera (the output never changes).
        if let Some(camera) = camera {
            rc.camera = Some(camera);
            if let Some(blur) = &mut rc.motion_blur {
                let blur = std::sync::Arc::make_mut(blur);
                blur.cameras.fill(Some(camera));
            }
        }
        // sampleImage: the layers expressions sampled are drawn and read back
        // without waiting. A frame evaluated before its samples arrived is not
        // shown: the previous image stays up and the viewer evaluates again.
        if self.samples.has_requests()
            && let Err(e) = self.engine.read_samples(stage, &rc, &self.samples)
        {
            tracing_warn(&format!("preview: sampleImage: {e}"));
        }
        let arrived = self.engine.renderer.poll_samples(&self.samples);
        self.waiting = arrived || self.samples.is_waiting();
        if self.waiting && self.rect.is_some() {
            return Some(PreviewImage {
                handle: self.registry.display_handle(self.id)?,
                rect: self.rect?,
            });
        }
        let key = (self.engine.renderer.content_hash(&rc), rc.time, resolution);
        let view = (self.display, self.roi);
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
            let (target, rect) = rendered;
            self.preparing = rect.is_none();
            let Some(rect) = rect else {
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
            self.last = Some(key);
            self.rect = Some(rect);
            self.target = Some(target);
            self.target_shown = true;
            self.sampled = None;
        }
        // Back from a snapshot: show the frame again.
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
        }
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect?,
        })
    }

    /// Take Snapshot: keep the frame shown now in `slot` (0–3).
    pub fn take_snapshot(&mut self, slot: usize) -> bool {
        let (Some(r), Some(s)) = (self.rect, self.snapshots.get_mut(slot)) else {
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
    pub fn show_snapshot(&mut self, slot: usize) -> Option<PreviewImage> {
        let (t, rect) = self.snapshots.get(slot)?.clone()?;
        self.registry
            .publish(self.id, &t, wgpu::FilterMode::Linear)
            .ok()?;
        self.target_shown = false;
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect,
        })
    }

    /// Straight (unpremultiplied) 8-bit RGBA of the published frame at comp
    /// point `p`, as shown (display-encoded); `None` outside the frame. Reads
    /// one texel back from the GPU (blocking, only when the pointer moved to
    /// another texel or the frame changed).
    pub fn pixel(&mut self, p: [f64; 2]) -> Option<[u8; 4]> {
        let (texture, rect) = (self.target.as_ref()?.view().texture(), self.rect?);
        let (w, h) = (texture.width(), texture.height());
        let (rw, rh) = (f64::from(rect.x1 - rect.x0), f64::from(rect.y1 - rect.y0));
        if rw <= 0.0 || rh <= 0.0 {
            return None;
        }
        let u = (p[0] - f64::from(rect.x0)) / rw;
        let v = (p[1] - f64::from(rect.y0)) / rh;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let texel = (
            ((u * f64::from(w)) as u32).min(w - 1),
            ((v * f64::from(h)) as u32).min(h - 1),
            self.last.map_or(0, |k| k.0 ^ u64::from(k.2)),
        );
        if let Some((t, c)) = self.sampled
            && t == texel
        {
            return Some(c);
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
        let image = readback.map().wait(&self.gpu).ok()?;
        let px = image.pixel(0, 0);
        let a = px[3];
        let straight = |c: u8| {
            if a == 0 {
                0
            } else {
                ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
            }
        };
        let c = [straight(px[0]), straight(px[1]), straight(px[2]), a];
        self.sampled = Some((texel, c));
        Some(c)
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
