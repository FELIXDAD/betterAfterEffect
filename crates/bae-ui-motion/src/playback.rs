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
use bae_render::{AnimEvaluator, IRect, RenderPurpose};
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

#[derive(Default)]
struct Mailbox {
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
}

impl PlaybackPreview {
    pub(super) fn new(gpu: Arc<GpuContext>, registry: Arc<ViewportTextureRegistry>) -> Self {
        Self {
            shared: Arc::default(),
            shown: None,
            gpu,
            registry,
            running: false,
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
            sources: sources.map(|s| s.0.generation()),
            effects: effects.map(|e| e.0.generation()),
        };
        let mut state = self.shared.state.lock().unwrap();
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

/// A frame evaluated and converted, waiting to be rendered.
struct Job {
    epoch: u64,
    scene: Arc<Scene>,
    stage: Arc<Stage>,
    time: Time,
    rc: bae_render::RenderComp,
    errors: Vec<AttrError>,
}

/// The pipeline's first stage: evaluates and converts frames ahead.
struct Evaluate {
    scene: Arc<Scene>,
    stage: Arc<Stage>,
    eval: Evaluator,
    anim: AnimEvaluator,
}

impl Evaluate {
    fn new(scene: Arc<Scene>) -> Result<Self, String> {
        Ok(Self {
            stage: Arc::new(scene.snapshot.restore()?),
            eval: Evaluator::new(),
            anim: AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview),
            scene,
        })
    }

    fn frame(&mut self, epoch: u64, time: Time) -> Result<Job, String> {
        let comp = self
            .eval
            .evaluate_comp(&self.stage, &self.scene.key.path, time)
            .map_err(|e| e.to_string())?;
        let rc = self
            .anim
            .convert(&self.stage, &comp, 0)
            .map_err(|e| e.to_string())?;
        Ok(Job {
            epoch,
            scene: self.scene.clone(),
            stage: self.stage.clone(),
            time,
            rc,
            errors: comp.errors,
        })
    }
}

/// Registers `scene`'s layer sources and effects with `engine`.
fn sync_engine(engine: &mut RenderEngine, scene: &Scene) {
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
                    && let Some(&time) = d.times.iter().find(|t| {
                        !state.ready.iter().any(|s| s.time == **t)
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
                current = Some(Evaluate::new(scene.clone())?);
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
        let result = engine
            .render(&job.stage, &job.rc, job.scene.key.resolution)
            .map_err(|e| e.to_string())
            // Not an interactive thread: it waits for the warm-up instead.
            .and_then(|r| r.ok_or_else(|| "incomplete frame".to_owned()))
            .and_then(|(texture, rect)| {
                let slot = Arc::new(Slot {
                    registry: registry.clone(),
                    id: registry.create("playback frame"),
                    rect,
                    time: job.time,
                    errors: job.errors,
                });
                registry
                    .publish(slot.id, &texture, wgpu::FilterMode::Linear)
                    .map_err(|e| e.to_string())?;
                Ok(slot)
            });
        let (epoch, time) = (job.epoch, job.time);
        match result {
            Ok(slot) => {
                in_flight = true;
                let shared = shared.clone();
                gpu.queue().on_submitted_work_done(move || {
                    let mut state = shared.state.lock().unwrap();
                    if state.epoch != epoch || state.shutdown {
                        return;
                    }
                    state.queued.retain(|t| *t != time);
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
