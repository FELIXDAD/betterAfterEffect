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

struct Producer {
    stage: Stage,
    eval: Evaluator,
    engine: RenderEngine,
    scene: Arc<Scene>,
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
        Ok(Self {
            stage: scene.snapshot.restore()?,
            eval: Evaluator::new(),
            engine,
            scene,
        })
    }

    fn render(
        &mut self,
        time: Time,
        registry: &Arc<ViewportTextureRegistry>,
    ) -> Result<Arc<Slot>, String> {
        let comp = self
            .eval
            .evaluate_comp(&self.stage, &self.scene.key.path, time)
            .map_err(|e| e.to_string())?;
        let rc = self
            .engine
            .anim
            .convert(&self.stage, &comp, 0)
            .map_err(|e| e.to_string())?;
        let (texture, rect) = self
            .engine
            .render(&self.stage, &rc, self.scene.key.resolution)
            .map_err(|e| e.to_string())?
            // Not an interactive thread: it waits for the warm-up instead.
            .ok_or("incomplete frame")?;
        let slot = Arc::new(Slot {
            registry: registry.clone(),
            id: registry.create("playback frame"),
            rect,
            time,
            errors: comp.errors,
        });
        registry
            .publish(slot.id, &texture, wgpu::FilterMode::Linear)
            .map_err(|e| e.to_string())?;
        Ok(slot)
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
                        !state.ready.iter().any(|s| s.time == **t)
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
            Ok(slot) => {
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
