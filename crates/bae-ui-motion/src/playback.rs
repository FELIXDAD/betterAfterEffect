//! The composition viewer's producer and consumer share only the current
//! request and finished display textures. Rendering never holds the mailbox.
//!
//! The producer is a two-stage pipeline: one thread evaluates and converts
//! frame N+1 while another encodes and submits frame N, so playback runs at
//! the slower stage instead of their sum. The UI thread only shows frames the
//! GPU has finished.

use crate::gpu_preview::{PreviewEffects, PreviewImage, PreviewSources, RenderEngine};
use crate::preview_snapshot::PreviewSnapshot;
use bae_anim::{AttrError, Evaluator};
use bae_app::Resources;
use bae_gpu::{
    GpuContext,
    viewport::{ViewportTextureId, ViewportTextureRegistry},
    wgpu,
};
use bae_model::{FrameRate, PrimPath, Stage, Time, TimeRange};
use bae_render::cache::{FrameCache, FrameKey};
use bae_render::{AnimEvaluator, IRect, OutputTarget, RenderPurpose};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const DEPTH: usize = 3;

#[derive(Clone, PartialEq, Eq)]
struct SceneKey {
    revision: u64,
    path: PrimPath,
    resolution: u32,
    nested_switches: bool,
    scope_source: bool,
    sources: Option<u64>,
    effects: Option<u64>,
}

impl SceneKey {
    fn frame(&self, time: Time) -> FrameKey {
        FrameKey {
            comp: self.path.clone(),
            time,
            request: u64::from(self.resolution) | (u64::from(self.nested_switches) << 32),
        }
    }

    fn same_view(&self, other: &Self) -> bool {
        self.path == other.path
            && self.resolution == other.resolution
            && self.nested_switches == other.nested_switches
            && self.scope_source == other.scope_source
            && self.sources == other.sources
            && self.effects == other.effects
    }
}

struct Scene {
    key: SceneKey,
    snapshot: PreviewSnapshot,
    sources: Option<PreviewSources>,
    effects: Option<PreviewEffects>,
}

struct Slot {
    channels: Option<Arc<crate::gpu_preview::ChannelFrame>>,
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    rect: IRect,
    time: Time,
    errors: Vec<AttrError>,
    bytes: u64,
    source: Option<bae_render::GpuImage>,
    /// The published texture (the Info panel and snapshots read it).
    texture: wgpu::Texture,
    /// Its target, handed to [`Spares`] when the frame is dropped.
    target: Option<OutputTarget>,
    spares: Arc<Mutex<Spares>>,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.registry.remove(self.id);
        if let Some(target) = self.target.take() {
            self.spares.lock().unwrap().give(target);
        }
    }
}

impl Slot {
    fn image(&self) -> Option<PreviewImage> {
        Some(PreviewImage {
            handle: self.registry.display_handle(self.id)?,
            rect: self.rect,
        })
    }
}

/// Spare output targets kept at most (a 4K target is 33 MB).
const MAX_SPARES: usize = 6;
/// Spare targets allocated while the viewer is idle, so live playback renders
/// into them instead of allocating.
const PREALLOC_SPARES: usize = 4;
/// A dropped frame's target is reused after this: the viewer no longer
/// draws it.
const REUSE_AFTER: Duration = Duration::from_millis(100);

/// Output targets of dropped frames, and targets allocated while idle,
/// rendered into again instead of allocating a whole frame during playback:
/// allocating a 4K target there stalled the first live frame after the
/// prefetched ones by 18–20 ms (the driver growing its heap).
#[derive(Default)]
struct Spares {
    targets: VecDeque<(Instant, OutputTarget)>,
    /// The frame size spares are made in (the last frame rendered).
    size: Option<[u32; 2]>,
}

impl Spares {
    fn give(&mut self, target: OutputTarget) {
        if self.targets.len() < MAX_SPARES && Some(target.size()) == self.size {
            self.targets.push_back((Instant::now(), target));
        }
    }

    /// A spare the viewer no longer draws.
    fn take(&mut self) -> Option<OutputTarget> {
        let size = self.size?;
        self.targets.retain(|(_, t)| t.size() == size);
        if self.targets.front()?.0.elapsed() < REUSE_AFTER {
            return None;
        }
        self.targets.pop_front().map(|(_, t)| t)
    }

    /// The size to allocate another spare in, while fewer than
    /// [`PREALLOC_SPARES`] are ready.
    fn wanted(&self) -> Option<[u32; 2]> {
        (self.targets.len() < PREALLOC_SPARES)
            .then_some(self.size)
            .flatten()
    }

    /// A target allocated ahead (usable at once).
    fn add(&mut self, target: OutputTarget) {
        let ready = Instant::now()
            .checked_sub(REUSE_AFTER)
            .unwrap_or_else(Instant::now);
        self.targets.push_back((ready, target));
    }
}

struct Demand {
    scene: Arc<Scene>,
    times: Vec<Time>,
    /// Frames to render into the cache once `times` are done (the first frames
    /// playback would show, prepared while the viewer is idle).
    prefetch: Vec<Time>,
    range: TimeRange,
    looping: bool,
    rate: FrameRate,
    repaint: egui::Context,
}

struct DisplayCache {
    frames: FrameCache<Arc<Slot>>,
    scene: Option<SceneKey>,
    revision: u64,
    dynamic: bool,
}

impl Default for DisplayCache {
    fn default() -> Self {
        Self {
            frames: FrameCache::new(1024 << 20),
            scene: None,
            revision: 1,
            dynamic: false,
        }
    }
}

impl DisplayCache {
    fn sync(&mut self, stage: &Stage, key: &SceneKey, budget: u64) {
        if self.frames.stats().budget != budget {
            self.frames.set_budget(budget);
            self.revision += 1;
        }
        if self.scene.as_ref() == Some(key) {
            return;
        }
        let previous = self.scene.replace(key.clone());
        if !self.dynamic
            && let Some(old) = previous
            && old.same_view(key)
            && let Some(changes) = stage.changes_since(old.revision)
        {
            for event in changes {
                self.frames.invalidate(event);
            }
        } else {
            self.frames.clear();
        }
        self.dynamic = false;
        let _ = stage.changes_since(stage.revision());
        self.revision += 1;
    }
}

#[derive(Default)]
struct Mailbox {
    cache: DisplayCache,
    demand: Option<Demand>,
    ready: VecDeque<Arc<Slot>>,
    queued: Vec<Time>,
    failed: Vec<Time>,
    epoch: u64,
    shutdown: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<Mailbox>,
    changed: Condvar,
}

pub(super) struct PlaybackPreview {
    shared: Arc<Shared>,
    shown: Option<Arc<Slot>>,
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    running: bool,
    ranges_revision: u64,
}

impl PlaybackPreview {
    pub(super) fn new(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        Self {
            shared: Arc::default(),
            shown: None,
            gpu,
            registry,
            running: false,
            ranges_revision: 0,
        }
    }

    pub(super) fn publish_ranges(
        &mut self,
        session: &crate::session::SharedSession,
        rate: FrameRate,
    ) {
        let state = self.shared.state.lock().unwrap();
        if self.ranges_revision == state.cache.revision {
            return;
        }
        self.ranges_revision = state.cache.revision;
        let Some(scene) = &state.cache.scene else {
            return;
        };
        let mut spans: Vec<TimeRange> = Vec::new();
        for time in state.cache.frames.cached_times(&scene.path) {
            let end = rate.frame_to_time(rate.time_to_frame(time) + 1);
            if let Some(last) = spans.last_mut()
                && last.end == time
            {
                last.end = end;
            } else {
                spans.push(TimeRange::new(time, end));
            }
        }
        session.lock().cached_preview = Some((scene.path.clone(), scene.revision, spans));
    }

    /// Renders `times` into the cache after the demanded frames (the frames
    /// playback would start with, while the viewer is idle).
    pub(super) fn prefetch(&self, times: Vec<Time>) {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(d) = state.demand.as_mut()
            && d.prefetch != times
        {
            d.prefetch = times;
            self.shared.changed.notify_all();
        }
    }

    pub(super) fn stop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        if state.demand.take().is_some() {
            state.epoch += 1;
            state.ready.clear();
            state.queued.clear();
            state.failed.clear();
            self.shown = None;
            self.shared.changed.notify_one();
        }
    }

    /// Returns the newest completed frame at or before the playhead. A slow
    /// producer can finish a late frame while the clock keeps advancing.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn poll(
        &mut self,
        stage: &Stage,
        resources: &Resources,
        path: &PrimPath,
        time: Time,
        rate: FrameRate,
        range: TimeRange,
        looping: bool,
        resolution: u32,
        repaint: &egui::Context,
    ) -> Option<(PreviewImage, Time, &[AttrError])> {
        if !self.running {
            let (gpu, registry, shared) =
                (self.gpu.clone(), self.registry.clone(), self.shared.clone());
            std::thread::Builder::new()
                .name("bae-playback".into())
                .spawn(move || worker(gpu, registry, shared))
                .expect("spawn playback worker");
            self.running = true;
        }
        let sources = resources.get::<PreviewSources>();
        let effects = resources.get::<PreviewEffects>();
        let key = SceneKey {
            revision: stage.revision(),
            path: path.clone(),
            resolution,
            nested_switches: crate::prefs::current(resources)
                .bool("composition.switches_affect_nested_comps"),
            scope_source: crate::scopes::needs_working_frame(repaint),
            sources: sources.map(|s| s.0.generation()),
            effects: effects.map(|e| e.0.generation()),
        };
        let mut state = self.shared.state.lock().unwrap();
        let (budget_mib, reserved_gb) =
            resources
                .get::<Arc<bae_prefs::PrefsStore>>()
                .map_or((1024, 0), |store| {
                    let prefs = store.current();
                    (
                        prefs.int(bae_prefs::keys::PREVIEW_CACHE_MIB),
                        prefs.int(bae_prefs::keys::RAM_RESERVED_GB),
                    )
                });
        state.cache.sync(
            stage,
            &key,
            cache_budget(budget_mib, reserved_gb, bae_platform::memory::total_bytes()),
        );
        let frame_key = key.frame(time);
        let replaced = state.demand.as_ref().is_none_or(|d| d.scene.key != key);
        if replaced {
            state.epoch += 1;
            state.queued.clear();
            state.ready.clear();
            state.failed.clear();
            self.shown = None;
            state.demand = Some(Demand {
                scene: Arc::new(Scene {
                    key,
                    snapshot: PreviewSnapshot::take(stage),
                    sources: sources.cloned(),
                    effects: effects.cloned(),
                }),
                times: Vec::with_capacity(DEPTH),
                prefetch: Vec::new(),
                range,
                looping,
                rate,
                repaint: repaint.clone(),
            });
        }
        let d = state.demand.as_mut().unwrap();
        let discontinuity = d.times.first().is_some_and(|old| time < *old)
            || d.range != range
            || d.looping != looping
            || d.rate != rate;
        if discontinuity || d.times.first() != Some(&time) {
            d.range = range;
            d.looping = looping;
            d.rate = rate;
            d.times.clear();
            let mut frame = rate.time_to_frame(time);
            for _ in 0..DEPTH {
                let mut t = rate.frame_to_time(frame);
                if t >= range.end {
                    if !looping {
                        break;
                    }
                    frame = rate.time_to_frame(range.start);
                    t = rate.frame_to_time(frame);
                }
                if d.times.contains(&t) {
                    break;
                }
                d.times.push(t);
                frame += 1;
            }
            let times = d.times.clone();
            if discontinuity {
                // Jobs already encoded can finish, but cannot publish into the
                // new epoch. Completed frames inside the new horizon are reusable.
                d.prefetch.clear();
                state.epoch += 1;
                state.queued.clear();
                state.ready.retain(|slot| times.contains(&slot.time));
                self.shown = None;
            }
            state.failed.retain(|t| times.contains(t));
            self.shared.changed.notify_one();
        }
        if let Some(slot) = state
            .cache
            .frames
            .get(&frame_key)
            .filter(|s| s.image().is_some())
        {
            self.shown = Some(slot.clone());
        } else if let Some(slot) = state
            .ready
            .iter()
            .filter(|s| {
                range.contains(s.time)
                    && s.time <= time
                    && self.shown.as_ref().is_none_or(|shown| s.time >= shown.time)
                    && s.image().is_some()
            })
            .max_by_key(|s| s.time)
        {
            self.shown = Some(slot.clone());
        }
        let registering = state
            .ready
            .iter()
            .any(|s| s.time == time && s.image().is_none());
        drop(state);
        if registering {
            repaint.request_repaint();
        }
        let shown = self.shown.as_ref()?;
        Some((shown.image()?, shown.time, &shown.errors))
    }

    pub(super) fn scope_source(&self) -> Option<bae_render::GpuImage> {
        self.shown.as_ref()?.source.clone()
    }

    /// The texture and rect of the frame [`Self::poll`] returned last.
    pub(super) fn shown_channels(&self) -> Option<Arc<crate::gpu_preview::ChannelFrame>> {
        self.shown.as_ref()?.channels.clone()
    }

    pub(super) fn shown_texture(&self) -> Option<(wgpu::Texture, IRect)> {
        let shown = self.shown.as_ref()?;
        Some((shown.texture.clone(), shown.rect))
    }
}

impl Drop for PlaybackPreview {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().shutdown = true;
        self.shared.changed.notify_one();
    }
}

/// A frame evaluated and converted, waiting to be rendered.
struct Job {
    epoch: u64,
    scene: Arc<Scene>,
    stage: Arc<Stage>,
    time: Time,
    rc: bae_render::RenderComp,
    errors: Vec<AttrError>,
    /// The project has expressions beyond time arithmetic (or linked
    /// projects): cached frames cannot be invalidated selectively.
    dynamic: bool,
}

/// Recognizes only the numeric/time subset needed for dependency tracking.
fn time_arithmetic(source: &str) -> bool {
    source.split("time").all(|part| {
        part.bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_whitespace() || b"+-*/%().".contains(&b))
    })
}

/// The pipeline's first stage: evaluates and converts frames ahead.
struct Evaluate {
    scene: Arc<Scene>,
    stage: Arc<Stage>,
    eval: Evaluator,
    anim: AnimEvaluator,
    gpu: Arc<GpuContext>,
    /// Layer pixels for `sampleImage`, and the renderer that reads them back
    /// (made when an expression first samples).
    samples: Arc<bae_anim::ImageSamples>,
    sampler: Option<RenderEngine>,
    dynamic: bool,
}

/// Rounds of `sampleImage` read-backs per frame (a sampled layer whose own
/// pixels come from samples needs one more).
const SAMPLE_ROUNDS: usize = 4;

impl Evaluate {
    fn new(scene: Arc<Scene>, gpu: Arc<GpuContext>) -> Result<Self, String> {
        let stage = scene.snapshot.restore()?;
        // Pure arithmetic over time has no project reads. General JavaScript
        // can look up other compositions or sample outside this frame.
        let dynamic = stage.sublayers().any(|l| {
            l.paths().any(|p| {
                l.spec(p).is_some_and(|s| {
                    s.attrs.values().any(|a| {
                        a.expression
                            .as_ref()
                            .is_some_and(|e| e.enabled && !time_arithmetic(&e.source))
                    })
                })
            })
        }) || stage.attached_projects().next().is_some();
        let samples = Arc::new(bae_anim::ImageSamples::new());
        let mut eval = Evaluator::new();
        eval.set_image_samples(Some(samples.clone()));
        let mut anim = AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview);
        anim.set_nested_switches(scene.key.nested_switches);
        Ok(Self {
            dynamic,
            stage: Arc::new(stage),
            eval,
            anim,
            gpu,
            samples,
            sampler: None,
            scene,
        })
    }

    fn evaluate(
        &mut self,
        time: Time,
    ) -> Result<(bae_anim::EvaluatedComp, bae_render::RenderComp), String> {
        let comp = self
            .eval
            .evaluate_comp(&self.stage, &self.scene.key.path, time)
            .map_err(|e| e.to_string())?;
        let rc = self
            .anim
            .convert(&self.stage, &comp, 0)
            .map_err(|e| e.to_string())?;
        Ok((comp, rc))
    }

    fn frame(&mut self, epoch: u64, time: Time) -> Result<Job, String> {
        let (mut comp, mut rc) = self.evaluate(time)?;
        // sampleImage: not an interactive thread, so it waits for the sampled
        // pixels (as export does) and evaluates again.
        for _ in 0..SAMPLE_ROUNDS {
            if !self.samples.has_requests() {
                break;
            }
            let sampler = match &mut self.sampler {
                Some(s) => s,
                None => {
                    let mut engine = RenderEngine::new(self.gpu.clone(), 0);
                    sync_engine(&mut engine, &self.scene);
                    self.sampler.insert(engine)
                }
            };
            sampler
                .read_samples(&self.stage, &rc, &self.samples)
                .and_then(|()| sampler.renderer.wait_samples(&self.samples))
                .map_err(|e| e.to_string())?;
            (comp, rc) = self.evaluate(time)?;
        }
        Ok(Job {
            epoch,
            scene: self.scene.clone(),
            stage: self.stage.clone(),
            time,
            rc,
            errors: comp.errors,
            dynamic: self.dynamic,
        })
    }
}

/// Registers `scene`'s layer sources and effects with `engine`.
fn sync_engine(engine: &mut RenderEngine, scene: &Scene) {
    engine.anim.set_nested_switches(scene.key.nested_switches);
    if let Some(sources) = &scene.sources {
        for entry in sources.0.snapshot().iter() {
            engine
                .renderer
                .providers_mut()
                .register(entry.item().clone());
        }
    }
    if let Some(effects) = &scene.effects {
        engine.renderer.sync_effects(&effects.0);
    }
}

/// Evaluates frame N+1 while the render thread encodes and submits frame N:
/// playback runs at the slower of the two stages, not their sum.
fn worker(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>, shared: Arc<Shared>) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Job>(1);
    {
        let (gpu, shared) = (gpu.clone(), shared.clone());
        std::thread::Builder::new()
            .name("bae-playback-render".into())
            .spawn(move || render_worker(gpu, registry, shared, rx))
            .expect("spawn playback render thread");
    }
    let mut current: Option<Evaluate> = None;
    loop {
        let (epoch, scene, time) = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if state.shutdown || gpu.is_lost() {
                    return;
                }
                if let Some(d) = &state.demand
                    && state.queued.len() < DEPTH
                    && let Some(&time) = d.times.iter().chain(&d.prefetch).find(|t| {
                        !state.cache.frames.contains(&d.scene.key.frame(**t))
                            && !state.ready.iter().any(|s| s.time == **t)
                            && !state.queued.contains(t)
                            && !state.failed.contains(t)
                    })
                {
                    let scene = d.scene.clone();
                    state.queued.push(time);
                    break (state.epoch, scene, time);
                }
                // Frames finishing on the GPU or a new demand wake us.
                state = shared.changed.wait(state).unwrap();
            }
        };
        let result = (|| {
            if current
                .as_ref()
                .is_none_or(|c| !Arc::ptr_eq(&c.scene, &scene))
            {
                current = Some(Evaluate::new(scene.clone(), gpu.clone())?);
            }
            current.as_mut().unwrap().frame(epoch, time)
        })();
        match result {
            Ok(job) => {
                if tx.send(job).is_err() {
                    return;
                }
            }
            Err(error) => failed(&shared, epoch, time, &error),
        }
    }
}

fn failed(shared: &Shared, epoch: u64, time: Time, error: &str) {
    tracing::warn!("playback: {error}");
    let mut state = shared.state.lock().unwrap();
    if state.epoch == epoch {
        state.failed.push(time);
        state.queued.retain(|t| *t != time);
    }
}

/// The pipeline's second stage: encodes and submits converted frames; a
/// frame is published when the GPU has finished it.
/// Whether the viewer is idle (not playing): it then demands only its
/// current frame.
fn idle(shared: &Shared) -> bool {
    shared
        .state
        .lock()
        .unwrap()
        .demand
        .as_ref()
        .is_none_or(|d| d.range.duration() <= d.rate.frame_duration())
}

fn render_worker(
    gpu: Arc<GpuContext>,
    registry: Arc<ViewportTextureRegistry>,
    shared: Arc<Shared>,
    rx: std::sync::mpsc::Receiver<Job>,
) {
    use std::sync::mpsc::TryRecvError;
    // One renderer for the whole playback (its pipelines, texture pool and
    // remembered vector images survive edits); sources and effects are
    // registered again when their registries changed.
    let mut engine = RenderEngine::new(gpu.clone(), 0);
    let mut synced: Option<(Option<u64>, Option<u64>)> = None;
    let mut in_flight = false;
    let spares: Arc<Mutex<Spares>> = Arc::default();
    loop {
        let job = match rx.try_recv() {
            Ok(job) => job,
            Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {
                // Nothing to encode: let the submitted frames finish (their
                // callbacks publish them), then wait for the next one.
                if in_flight {
                    if gpu.wait_idle().is_err() {
                        return;
                    }
                    in_flight = false;
                }
                // Idle viewer: allocate the targets playback will render into.
                if idle(&shared) {
                    loop {
                        let wanted = spares.lock().unwrap().wanted();
                        let Some(size) = wanted else {
                            break;
                        };
                        let target = OutputTarget::new(gpu.device(), size);
                        spares.lock().unwrap().add(target);
                    }
                }
                match rx.recv() {
                    Ok(job) => job,
                    Err(_) => return,
                }
            }
        };
        // Superseded by a seek, an edit or a stop while it waited.
        if shared.state.lock().unwrap().epoch != job.epoch {
            continue;
        }
        let generations = (job.scene.key.sources, job.scene.key.effects);
        if synced != Some(generations) {
            sync_engine(&mut engine, &job.scene);
            synced = Some(generations);
        }
        let spare = spares.lock().unwrap().take();
        let result = engine
            .render(
                &job.stage,
                &job.rc,
                job.scene.key.resolution,
                bae_render::Display::default(),
                None,
                spare,
            )
            .map_err(|e| e.to_string())
            .and_then(|(target, source, temporal)| {
                let mut spares_now = spares.lock().unwrap();
                spares_now.size = Some(target.size());
                // Not an interactive thread: it waits for the warm-up instead.
                let Some(source) = source else {
                    spares_now.give(target);
                    return Err("incomplete frame".to_owned());
                };
                drop(spares_now);
                let rect = source.rect;
                let source = job.scene.key.scope_source.then_some(source);
                let texture = target.view().texture().clone();
                let channels = engine.channels.clone();
                let pick_bytes = channels
                    .as_ref()
                    .map_or(0, |c| c.layers.values().map(|l| l.pick_bytes()).sum());
                let slot = Arc::new(Slot {
                    channels,
                    registry: registry.clone(),
                    id: registry.create("playback frame"),
                    rect,
                    time: job.time,
                    errors: job.errors,
                    bytes: u64::from(texture.width()) * u64::from(texture.height()) * 4
                        + pick_bytes
                        + source.as_ref().map_or(0, bae_render::GpuImage::byte_size),
                    source,
                    texture,
                    target: Some(target),
                    spares: spares.clone(),
                });
                registry
                    .publish(slot.id, &slot.texture, wgpu::FilterMode::Linear)
                    .map_err(|e| e.to_string())?;
                Ok((slot, temporal))
            });
        let (epoch, time) = (job.epoch, job.time);
        // What the frame depends on (selective invalidation), unless it reads
        // other times, nested comps or motion-blur samples.
        let deps = job.rc.dependencies();
        let dynamic_rc = job.rc.motion_blur.is_some()
            || job
                .rc
                .layers
                .iter()
                .any(|l| matches!(l.content, bae_render::LayerContent::Comp(_)));
        let scene = job.scene.clone();
        match result {
            Ok((slot, temporal)) => {
                let dynamic = job.dynamic || temporal || dynamic_rc;
                in_flight = true;
                let shared = shared.clone();
                gpu.queue().on_submitted_work_done(move || {
                    let mut state = shared.state.lock().unwrap();
                    if state.epoch != epoch || state.shutdown {
                        return;
                    }
                    state.queued.retain(|t| *t != time);
                    let cache = &mut state.cache;
                    if slot.bytes <= cache.frames.stats().budget {
                        cache.frames.insert(
                            scene.key.frame(time),
                            slot.clone(),
                            slot.bytes,
                            deps,
                            0,
                        );
                        cache.dynamic |= dynamic;
                        cache.revision += 1;
                    }
                    state.ready.push_back(slot);
                    while state.ready.len() > DEPTH {
                        state.ready.pop_front();
                    }
                    let repaint = state.demand.as_ref().map(|d| d.repaint.clone());
                    drop(state);
                    if let Some(ctx) = repaint {
                        ctx.request_repaint();
                    }
                    shared.changed.notify_all();
                });
            }
            Err(error) => failed(&shared, epoch, time, &error),
        }
    }
}

/// The preview cache's byte budget: the GPU Preview Cache setting, never more
/// than the RAM left after Memory & CPU ▸ RAM Reserved for Other Applications.
fn cache_budget(cache_mib: i64, reserved_gb: i64, installed: Option<u64>) -> u64 {
    let budget = (cache_mib.max(0) as u64) << 20;
    installed.map_or(budget, |total| {
        budget.min(total.saturating_sub((reserved_gb.max(0) as u64) << 30))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_cache_leaves_the_reserved_ram_alone() {
        let gib = 1u64 << 30;
        assert_eq!(super::cache_budget(1024, 6, Some(16 * gib)), 1 << 30);
        assert_eq!(super::cache_budget(32768, 6, Some(16 * gib)), 10 * gib);
        assert_eq!(super::cache_budget(512, 64, Some(16 * gib)), 0);
        assert_eq!(super::cache_budget(512, 6, None), 512 << 20);
    }

    use super::*;

    #[test]
    fn spare_targets_wait_until_the_viewer_stopped_drawing_them() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let mut spares = Spares::default();
        assert!(spares.wanted().is_none(), "no frame size yet");
        spares.size = Some([64, 32]);
        // Allocated ahead: usable at once.
        while let Some(size) = spares.wanted() {
            spares.add(OutputTarget::new(gpu.device(), size));
        }
        assert_eq!(spares.targets.len(), PREALLOC_SPARES);
        let first = spares.take().expect("a spare");
        assert_eq!(first.size(), [64, 32]);
        // A dropped frame's target waits; another size is not kept.
        let mut fresh = Spares {
            size: Some([64, 32]),
            ..Spares::default()
        };
        fresh.give(first);
        fresh.give(OutputTarget::new(gpu.device(), [10, 10]));
        assert_eq!(fresh.targets.len(), 1);
        assert!(fresh.take().is_none(), "still drawn");
        std::thread::sleep(REUSE_AFTER);
        assert!(fresh.take().is_some());
    }
}
