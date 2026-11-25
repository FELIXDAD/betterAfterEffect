//! The composition viewer's producer and consumer share only the current
//! request and finished display textures. Rendering never holds the mailbox.

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
use bae_render::IRect;
use bae_render::cache::{FrameCache, FrameKey};
use bae_render::input::Dependency;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

const DEPTH: usize = 3;

#[derive(Clone, PartialEq, Eq)]
struct SceneKey {
    revision: u64,
    path: PrimPath,
    resolution: u32,
    sources: Option<u64>,
    effects: Option<u64>,
}

impl SceneKey {
    fn frame(&self, time: Time) -> FrameKey {
        FrameKey {
            comp: self.path.clone(),
            time,
            request: u64::from(self.resolution),
        }
    }

    fn same_view(&self, other: &Self) -> bool {
        self.path == other.path
            && self.resolution == other.resolution
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
    registry: Arc<ViewportTextureRegistry>,
    id: ViewportTextureId,
    rect: IRect,
    time: Time,
    errors: Vec<AttrError>,
    bytes: u64,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.registry.remove(self.id);
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

struct Demand {
    scene: Arc<Scene>,
    times: Vec<Time>,
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
            sources: sources.map(|s| s.0.generation()),
            effects: effects.map(|e| e.0.generation()),
        };
        let mut state = self.shared.state.lock().unwrap();
        let budget_mib = resources
            .get::<Arc<bae_prefs::PrefsStore>>()
            .map_or(1024, |store| {
                store.current().int(bae_prefs::keys::PREVIEW_CACHE_MIB)
            });
        state
            .cache
            .sync(stage, &key, (budget_mib.max(0) as u64) << 20);
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
}

impl Drop for PlaybackPreview {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().shutdown = true;
        self.shared.changed.notify_one();
    }
}

/// Recognizes only the numeric/time subset needed for dependency tracking.
fn time_arithmetic(source: &str) -> bool {
    source.split("time").all(|part| {
        part.bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_whitespace() || b"+-*/%().".contains(&b))
    })
}

struct Producer {
    stage: Stage,
    eval: Evaluator,
    engine: RenderEngine,
    scene: Arc<Scene>,
    dynamic: bool,
}

impl Producer {
    fn new(gpu: Arc<GpuContext>, scene: Arc<Scene>) -> Result<Self, String> {
        let mut engine = RenderEngine::new(gpu, 0);
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
        Ok(Self {
            stage,
            dynamic,
            eval: Evaluator::new(),
            engine,
            scene,
        })
    }

    fn render(
        &mut self,
        time: Time,
        registry: &Arc<ViewportTextureRegistry>,
    ) -> Result<(Arc<Slot>, Vec<Dependency>, bool), String> {
        let comp = self
            .eval
            .evaluate_comp(&self.stage, &self.scene.key.path, time)
            .map_err(|e| e.to_string())?;
        let rc = self
            .engine
            .anim
            .convert(&self.stage, &comp, 0)
            .map_err(|e| e.to_string())?;
        let (texture, rect, temporal) = self
            .engine
            .render(&self.stage, &rc, self.scene.key.resolution)
            .map_err(|e| e.to_string())?;
        let slot = Arc::new(Slot {
            registry: registry.clone(),
            id: registry.create("playback frame"),
            rect,
            time,
            errors: comp.errors,
            bytes: u64::from(texture.width()) * u64::from(texture.height()) * 4,
        });
        registry
            .publish(slot.id, &texture, wgpu::FilterMode::Linear)
            .map_err(|e| e.to_string())?;
        Ok((
            slot,
            rc.dependencies(),
            self.dynamic
                || temporal
                || rc.motion_blur.is_some()
                || rc
                    .layers
                    .iter()
                    .any(|l| matches!(l.content, bae_render::LayerContent::Comp(_))),
        ))
    }
}

fn worker(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>, shared: Arc<Shared>) {
    let mut producer: Option<Producer> = None;
    loop {
        let (epoch, scene, time) = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if state.shutdown || gpu.is_lost() {
                    return;
                }
                if let Some(d) = &state.demand
                    && state.queued.len() < DEPTH
                    && let Some(&time) = d.times.iter().find(|t| {
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
                if state.queued.is_empty() {
                    state = shared.changed.wait(state).unwrap();
                } else {
                    drop(state);
                    if gpu.wait_idle().is_err() {
                        return;
                    }
                    state = shared.state.lock().unwrap();
                }
            }
        };
        let result = (|| {
            if producer
                .as_ref()
                .is_none_or(|p| !Arc::ptr_eq(&p.scene, &scene))
            {
                producer = Some(Producer::new(gpu.clone(), scene.clone())?);
            }
            producer.as_mut().unwrap().render(time, &registry)
        })();
        match result {
            Ok((slot, deps, dynamic)) => {
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
                    shared.changed.notify_one();
                });
            }
            Err(error) => {
                tracing::warn!("playback: {error}");
                let mut state = shared.state.lock().unwrap();
                if state.epoch == epoch {
                    state.failed.push(time);
                    state.queued.retain(|t| *t != time);
                }
            }
        }
    }
}
