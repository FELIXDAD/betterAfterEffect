mod stabilize;
use super::gpu::{Ready, Region, TrackingGpu};
use crate::gpu_preview::{PreviewEffects, PreviewSources};
use bae_anim::{Evaluator, glam::DVec2};
use bae_gpu::{GpuContext, wgpu};
use bae_model::{PrimPath, Stage, Time, Value};
use bae_motion::{
    query,
    tracker::{self, Frame, Sample},
};
use bae_render::{AnimEvaluator, OutputAlpha, OutputTarget, RenderPurpose, RenderRequest};
use bae_ui::PanelContext;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};

pub(super) enum Update {
    Frame(Frame),
    End(Result<(), String>),
}
pub(super) struct Job {
    pub updates: Receiver<Update>,
    pub revision: u64,
    pub tracker: PrimPath,
    pub stop: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub(super) fn start(
    cx: &PanelContext<'_>,
    ui: &egui::Context,
    tracker: &PrimPath,
    start: Time,
    step: i32,
    single: bool,
) -> Result<Job, String> {
    launch(
        cx,
        ui,
        tracker,
        Mode::Point {
            start,
            step,
            single,
        },
    )
}
pub(super) enum Mode {
    Point {
        start: Time,
        step: i32,
        single: bool,
    },
    Stabilize(i64),
}
pub(super) fn launch(
    cx: &PanelContext<'_>,
    ui: &egui::Context,
    tracker: &PrimPath,
    mode: Mode,
) -> Result<Job, String> {
    let gpu = cx
        .resources
        .get::<Arc<GpuContext>>()
        .and_then(|g| cx.resources.get::<Arc<TrackingGpu>>()?.ready(g))
        .ok_or("Tracker is warming up")?;
    let source = query::owning_layer(cx.stage, tracker).ok_or("Missing motion source")?;
    let comp = query::owning_comp(cx.stage, &source).ok_or("Missing composition")?;
    let snapshot = cx.stage.snapshot_reader();
    let (tx, updates) = mpsc::channel();
    let job = Job {
        updates,
        revision: cx.stage.revision(),
        tracker: tracker.clone(),
        stop: Arc::default(),
    };
    let stop = job.stop.clone();
    let sources = cx.resources.get::<PreviewSources>().cloned();
    let effects = cx.resources.get::<PreviewEffects>().cloned();
    let tracker = tracker.clone();
    let ui = ui.clone();
    std::thread::Builder::new()
        .name("motion-tracker".into())
        .spawn(move || {
            bae_gpu::warm_up::lower_priority();
            let result = (|| {
                let stage = snapshot().map_err(|e| e.to_string())?;
                let mut renderer = gpu.renderer.lock().map_err(|e| e.to_string())?;
                let renderer = Ready::renderer(&gpu.gpu, &mut renderer);
                if let Some(src) = sources {
                    for e in src.0.snapshot().iter() {
                        renderer.providers_mut().register(e.item().clone());
                    }
                }
                if let Some(fx) = effects {
                    renderer.sync_effects(&fx.0);
                }
                let mut eval = Evaluator::new();
                let mut anim = AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview);
                let info = query::comp_info(&stage, &comp).ok_or("Missing composition")?;
                let duration = info.frame_duration().0;
                let fields =
                    tracker::setting(&stage, &tracker, "track_fields") == Some(Value::Bool(true));
                // The frames' 8-bit targets: one no image holds any more is
                // rendered into again instead of allocating a whole layer.
                let mut targets: Vec<Arc<OutputTarget>> = Vec::new();
                let mut picture = |time: Time| -> Result<Image, String> {
                    let phase = time.0.rem_euclid(duration);
                    let field = if fields {
                        1 + u32::from(phase >= duration / 2)
                    } else {
                        0
                    };
                    let time = if fields { Time(time.0 - phase) } else { time };
                    let comp = eval
                        .evaluate_comp(&stage, &comp, time)
                        .map_err(|e| e.to_string())?;
                    let (one, min) = crate::layer_panel::alone(&comp, &source, 0)
                        .ok_or("Missing motion source")?;
                    let rc = anim.convert(&stage, &one, 0).map_err(|e| e.to_string())?;
                    let (frame, target) = loop {
                        if stop.load(Ordering::Relaxed) {
                            return Err("Tracking stopped".into());
                        }
                        let free = targets
                            .iter()
                            .position(|t| Arc::strong_count(t) == 1)
                            .and_then(|i| Arc::try_unwrap(targets.swap_remove(i)).ok());
                        // The frame and its conversion in one submission.
                        let (frame, target) = renderer
                            .render_frame_output(
                                &rc,
                                &RenderRequest::full(),
                                None,
                                OutputAlpha::Straight,
                                free,
                            )
                            .map_err(|e| e.to_string())?;
                        if let Some(warning) = frame.warnings.first() {
                            return Err(warning.clone());
                        }
                        let target = Arc::new(target);
                        targets.push(target.clone());
                        if !frame.incomplete {
                            break (frame, target);
                        }
                        drop(target);
                        std::thread::sleep(std::time::Duration::from_millis(16));
                    };
                    // Keep a few spare targets at most.
                    while targets.len() > MAX_TARGETS
                        && let Some(i) = targets.iter().position(|t| Arc::strong_count(t) == 1)
                    {
                        targets.swap_remove(i);
                    }
                    let origin = min
                        + DVec2::new(
                            f64::from(frame.image.rect.x0),
                            f64::from(frame.image.rect.y0),
                        );
                    Ok(Image {
                        target,
                        origin,
                        field,
                    })
                };
                let timing = query::layer_timing(&stage, &source);
                let lo = timing.in_point.min(timing.out_point).max(Time::ZERO);
                let hi = timing.in_point.max(timing.out_point).min(info.duration);
                let (start, step, single) = match mode {
                    Mode::Point {
                        start,
                        step,
                        single,
                    } => (start, step, single),
                    Mode::Stabilize(_) => (lo, 1, false),
                };
                let stride = Time(duration * i64::from(step) / if fields { 2 } else { 1 });
                let run = Analysis {
                    stage: &stage,
                    tracker: &tracker,
                    gpu: &gpu,
                    start,
                    stride,
                    lo,
                    hi,
                    single,
                    stop: &stop,
                };
                let emit = |frame| {
                    tx.send(Update::Frame(frame))
                        .map_err(|_| "Tracker closed".to_owned())?;
                    ui.request_repaint();
                    Ok(())
                };
                match mode {
                    Mode::Stabilize(quality) => run.stabilize(quality, &mut picture, emit),
                    Mode::Point { .. } => run.track(&mut picture, emit),
                }
            })();
            let _ = tx.send(Update::End(result));
            ui.request_repaint();
        })
        .map_err(|e| e.to_string())?;
    Ok(job)
}
/// Output targets the analysis keeps beyond those its images hold.
const MAX_TARGETS: usize = 4;

#[derive(Clone)]
struct Image {
    target: Arc<OutputTarget>,
    origin: DVec2,
    field: u32,
}

impl Image {
    fn texture(&self) -> &wgpu::Texture {
        self.target.view().texture()
    }
}
struct Point {
    path: PrimPath,
    center: DVec2,
    velocity: DVec2,
    reference: Image,
    ref_center: DVec2,
}
struct Analysis<'a> {
    stage: &'a Stage,
    tracker: &'a PrimPath,
    gpu: &'a Ready,
    start: Time,
    stride: Time,
    lo: Time,
    hi: Time,
    single: bool,
    stop: &'a AtomicBool,
}
impl Analysis<'_> {
    fn track(
        &self,
        picture: &mut impl FnMut(Time) -> Result<Image, String>,
        mut emit: impl FnMut(Frame) -> Result<(), String>,
    ) -> Result<(), String> {
        let stage = self.stage;
        let tracker = self.tracker;
        let get = |k| tracker::setting(stage, tracker, k);
        let token = |k| {
            get(k)
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        };
        let channel = match token("channel").as_str() {
            "rgb" => 0,
            "saturation" => 2,
            _ => 1,
        };
        let enhance = match token("enhance").as_str() {
            "blur" => 1,
            "sharpen" => 2,
            _ => 0,
        };
        let adapt = get("adapt") == Some(Value::Bool(true));
        let sub = get("subpixel") == Some(Value::Bool(true));
        let low = token("low_confidence");
        let threshold = get("threshold").and_then(|v| v.as_f64()).unwrap_or(80.0);
        let mut t = self.start;
        if t < self.lo || t >= self.hi || t + self.stride < self.lo || t + self.stride >= self.hi {
            return Err("No frame in this direction".into());
        }
        let mut previous = picture(t)?;
        let value = |p: &PrimPath, k, time| {
            tracker::vector_at(stage, p, k, time)
                .map(DVec2::from)
                .ok_or_else(|| format!("Missing {k}"))
        };
        let mut pts = Vec::new();
        let mut seed = Vec::new();
        for path in tracker::points(stage, tracker) {
            let center = value(&path, "feature_center", t)?;
            seed.push(Sample {
                center: center.to_array(),
                confidence: 100.,
                attach: Some((center + value(&path, "attach_offset", t)?).to_array()),
            });
            pts.push(Point {
                path,
                center,
                velocity: DVec2::ZERO,
                reference: previous.clone(),
                ref_center: center,
            });
        }
        let mut initial = Some(Frame {
            time: t,
            points: seed,
        });
        while !self.stop.load(Ordering::Relaxed) {
            let next = t + self.stride;
            if next < self.lo || next >= self.hi {
                break;
            }
            let image = picture(next)?;
            let mut regions = Vec::new();
            let fallback = low == "adapt" && !adapt && t != self.start;
            for alternate in 0..=usize::from(fallback) {
                for p in &pts {
                    let (reference, center) = if alternate == 1 {
                        (&previous, p.center)
                    } else {
                        (&p.reference, p.ref_center)
                    };
                    let grid = |v: DVec2| v.round().as_ivec2().to_array();
                    let size = |k| {
                        value(&p.path, k, t).map(|v| {
                            v.round()
                                .clamp(
                                    DVec2::splat(2.),
                                    DVec2::new(
                                        f64::from(image.texture().width().max(2)),
                                        f64::from(image.texture().height().max(2)),
                                    ),
                                )
                                .as_uvec2()
                                .to_array()
                        })
                    };
                    regions.push(Region {
                        reference: reference.texture(),
                        field: reference.field,
                        center: grid(center - reference.origin),
                        search_center: grid(
                            p.center + value(&p.path, "search_offset", t)? - image.origin,
                        ),
                        feature: size("feature_size")?,
                        search: size("search_size")?,
                    });
                }
            }
            let matches = self.gpu.matcher.analyze(
                &self.gpu.gpu,
                image.texture(),
                &regions,
                super::gpu::Options {
                    channel,
                    subpixel: sub,
                    field: image.field,
                    radius: get("enhance_radius")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(2.0)
                        .round()
                        .clamp(1., 32.) as u32,
                    enhance,
                },
            )?;
            let mut samples = Vec::new();
            let n = pts.len();
            for (i, p) in pts.iter_mut().enumerate() {
                let mut hit = matches[i];
                if hit[2] as f64 * 100. < threshold && fallback {
                    p.reference = previous.clone();
                    p.ref_center = p.center;
                    hit = matches[n + i];
                }
                let confidence = f64::from(hit[2]).clamp(0., 1.) * 100.;
                if confidence < threshold && low == "stop" {
                    return Err("Tracking stopped below the confidence threshold".into());
                }
                let predicted = confidence < threshold && low == "extrapolate";
                let center = if predicted {
                    p.center + p.velocity
                } else if hit[2] < -1. {
                    p.center
                } else {
                    let reference = p.ref_center - p.reference.origin;
                    let fraction = reference - reference.round();
                    image.origin + DVec2::new(f64::from(hit[0]), f64::from(hit[1])) + fraction
                };
                let attach = if predicted {
                    None
                } else {
                    Some((center + value(&p.path, "attach_offset", next)?).to_array())
                };
                samples.push(Sample {
                    center: center.to_array(),
                    confidence,
                    attach,
                });
                p.velocity = center - p.center;
                p.center = center;
                if adapt {
                    p.reference = image.clone();
                    p.ref_center = center;
                }
            }
            if let Some(seed) = initial.take() {
                emit(seed)?;
            }
            emit(Frame {
                time: next,
                points: samples,
            })?;
            t = next;
            let image_field = image.field;
            previous = image;
            if self.single && (image_field == 0 || image_field == 1) {
                break;
            }
        }
        Ok(())
    }
}
