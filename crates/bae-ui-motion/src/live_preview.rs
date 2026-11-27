//! Live preview while a value is dragged (#593): the newest uncommitted value
//! renders on a background thread. A newer request replaces one that has not
//! started, a request waits until the GPU finished the previous frame (no
//! queue builds up behind a slow comp), and the viewer shows the newest
//! finished frame without waiting for it.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use bae_gpu::{GpuContext, wgpu};
use bae_model::{PrimPath, Stage};
use bae_render::{IRect, OutputTarget, RenderComp};

use crate::gpu_preview::{PreviewEffects, PreviewSources, RenderEngine};
use crate::preview_snapshot::PreviewSnapshot;

/// The project version and the layer sources and effects a live frame
/// renders with (the same for the whole drag).
pub(crate) struct LiveScene {
    /// Stage revision and the sources' and effects' registry generations.
    pub key: (u64, Option<u64>, Option<u64>),
    pub nested_switches: bool,
    pub snapshot: PreviewSnapshot,
    pub sources: Option<PreviewSources>,
    pub effects: Option<PreviewEffects>,
}

/// One frame to render: the comp converted with the dragged value.
pub(crate) struct LiveJob {
    pub scene: Arc<LiveScene>,
    pub rc: RenderComp,
    /// The dragged values (effects reading other frames and motion-blur
    /// samples evaluate them too).
    pub overrides: Vec<bae_anim::AttrOverride>,
    pub resolution: u32,
    pub display: bae_render::Display,
    pub roi: Option<IRect>,
    pub measure: bool,
    /// The viewer's `sampleImage` cache: requests waiting in it are read back
    /// here (this thread may wait for the GPU), and other frames this job
    /// evaluates sample through it too.
    pub samples: Option<Arc<bae_anim::ImageSamples>>,
    /// Render the frame; `false` only reads the samples it needs (the viewer
    /// evaluates again with them and sends the frame then).
    pub render: bool,
}

/// A finished live frame.
pub(crate) struct LiveFrame {
    /// The target it is in. The next live frame renders into the same
    /// texture: only complete frames ever land in it, in GPU order.
    pub view: wgpu::TextureView,
    pub size: [u32; 2],
    /// `None`: incomplete (something still warming up), the target is
    /// untouched.
    pub source: Option<bae_render::GpuImage>,
    pub layer_times: Option<(PrimPath, Vec<(PrimPath, Duration)>)>,
}

#[derive(Default)]
struct State {
    pending: Option<LiveJob>,
    done: Option<LiveFrame>,
    shutdown: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

pub(crate) struct LivePreview {
    gpu: Arc<GpuContext>,
    shared: Arc<Shared>,
    running: bool,
}

impl LivePreview {
    pub(crate) fn new(gpu: Arc<GpuContext>) -> Self {
        Self {
            gpu,
            shared: Arc::default(),
            running: false,
        }
    }

    /// Render `job` next, dropping a request that has not started.
    pub(crate) fn submit(&mut self, job: LiveJob) {
        if !self.running {
            let (gpu, shared) = (self.gpu.clone(), self.shared.clone());
            let spawned = std::thread::Builder::new()
                .name("bae-live-preview".into())
                .spawn(move || worker(gpu, shared));
            if spawned.is_err() {
                return;
            }
            self.running = true;
        }
        self.shared.state.lock().unwrap().pending = Some(job);
        self.shared.wake.notify_one();
    }

    /// The newest frame finished since the last call.
    pub(crate) fn take(&self) -> Option<LiveFrame> {
        self.shared.state.lock().unwrap().done.take()
    }
}

impl Drop for LivePreview {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().shutdown = true;
        self.shared.wake.notify_one();
    }
}

fn worker(gpu: Arc<GpuContext>, shared: Arc<Shared>) {
    let mut engine = RenderEngine::new(gpu, 0);
    let mut scene: Option<(Arc<LiveScene>, Arc<Stage>)> = None;
    let mut target: Option<OutputTarget> = None;
    loop {
        let job = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(job) = state.pending.take() {
                    break job;
                }
                state = shared.wake.wait(state).unwrap();
            }
        };
        if scene.as_ref().is_none_or(|(s, _)| s.key != job.scene.key) {
            let Ok(stage) = job.scene.snapshot.restore() else {
                continue;
            };
            if let Some(sources) = &job.scene.sources {
                for entry in sources.0.snapshot().iter() {
                    engine
                        .renderer
                        .providers_mut()
                        .register(entry.item().clone());
                }
            }
            if let Some(effects) = &job.scene.effects {
                engine.renderer.sync_effects(&effects.0);
            }
            engine.anim.clear_compiled();
            engine.anim.set_nested_switches(job.scene.nested_switches);
            scene = Some((job.scene.clone(), Arc::new(stage)));
        }
        let stage = scene.as_ref().unwrap().1.clone();
        engine.anim.set_overrides(job.overrides);
        bae_render::CompEvaluator::set_image_samples(&mut engine.anim, job.samples.clone());
        if let Some(samples) = &job.samples
            && samples.has_requests()
        {
            let read = engine
                .read_samples(&stage, &job.rc, samples)
                .and_then(|()| engine.renderer.wait_samples(samples));
            if let Err(e) = read {
                tracing::warn!("live preview: sampleImage: {e}");
            }
            bae_gpu::warm_up::request_redraw();
        }
        if !job.render {
            continue;
        }
        engine.renderer.set_layer_times(job.measure);
        let rendered = engine.render(
            &stage,
            &job.rc,
            job.resolution,
            job.display,
            job.roi,
            target.take(),
        );
        let Ok((out, source, _)) = rendered else {
            continue;
        };
        let frame = LiveFrame {
            view: out.view().clone(),
            size: out.size(),
            source,
            layer_times: engine.layer_times.take(),
        };
        target = Some(out);
        // The next request starts once the GPU finished this frame: a slow
        // comp shows fewer values, it does not fall behind the pointer.
        if engine.renderer.wait_idle().is_err() {
            continue;
        }
        shared.state.lock().unwrap().done = Some(frame);
        bae_gpu::warm_up::request_redraw();
    }
}
