//! Background camera analysis runs independently of the active panel.
use super::{
    gpu::TrackingGpu,
    worker::{FrameInput, pictures},
};
use crate::gpu_preview::{PreviewEffects, PreviewSources};
use bae_app::{AppBuilder, Resources};
use bae_gpu::GpuContext;
use bae_model::{PrimPath, Time, Value};
use bae_motion::{
    camera_track::{self, Settings, Solution},
    query,
};
use bae_ui::{PanelContext, tr};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Default)]
struct Analysis {
    jobs: BTreeMap<PrimPath, Job>,
    finished: BTreeMap<PrimPath, (Settings, String)>,
}
type Shared = Arc<Mutex<Analysis>>;
struct Job {
    settings: Settings,
    stop: Arc<AtomicBool>,
    step: Arc<AtomicU8>,
    result: mpsc::Receiver<Result<Solution, String>>,
    revision: u64,
    sources: Vec<PrimPath>,
    comp: PrimPath,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
pub(super) fn register(app: &mut AppBuilder) {
    app.insert_resource(Shared::default());
    let _ = app.register_arc::<dyn bae_ui::DialogHost>("camera-tracker", Arc::new(Host));
    bae_ui::i18n::register(&[
        (
            "Analyzing in background (step 1 of 2)",
            "在后台分析（第 1 步，共 2 步）",
        ),
        (
            "Solving camera (step 2 of 2)",
            "正在解算摄像机（第 2 步，共 2 步）",
        ),
        ("Insufficient matching track points", "可匹配的跟踪点不足"),
        ("Could not recover camera depth", "无法求出摄像机深度"),
        (
            "Specify a horizontal angle of view between 0 and 180 degrees",
            "请指定大于 0° 且小于 180° 的水平视角",
        ),
        (
            "Track points changed size during analysis",
            "分析期间素材尺寸发生变化",
        ),
        ("Average Error", "平均误差"),
        ("Analyze Camera", "分析摄像机"),
        ("Camera solved", "摄像机解算完成"),
        // Also the point tracker's message when it is stopped.
        ("Tracking stopped", "跟踪已停止"),
    ]);
}
fn start(
    cx: &PanelContext<'_>,
    ui: &egui::Context,
    effect: &PrimPath,
    settings: Settings,
) -> Result<Option<Job>, String> {
    let Some(ready) = cx
        .resources
        .get::<Arc<GpuContext>>()
        .and_then(|g| cx.resources.get::<Arc<TrackingGpu>>()?.ready(g))
    else {
        return Ok(None);
    };
    let comp = query::owning_comp(cx.stage, effect).ok_or("Missing composition")?;
    let source = query::owning_layer(cx.stage, effect).ok_or("Missing motion source")?;
    let read = cx.stage.snapshot_reader();
    let sources = cx.resources.get::<PreviewSources>().cloned();
    let effects = cx.resources.get::<PreviewEffects>().cloned();
    let (send, result) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let step = Arc::new(AtomicU8::new(1));
    let mut inputs = vec![source.clone()];
    let mut n = 0;
    while n < inputs.len() {
        let path = &inputs[n];
        let mut next = query::comp_layers(cx.stage, path);
        if let Some(v) = cx
            .stage
            .prim(path)
            .and_then(|p| query::static_value(&p, "source"))
            && let Some(src) = v.as_prim_ref()
        {
            next.push(src.clone());
        }
        for p in next {
            if !inputs.contains(&p) {
                inputs.push(p);
            }
        }
        n += 1;
    }
    let job = Job {
        settings,
        stop: stop.clone(),
        step: step.clone(),
        result,
        revision: cx.stage.revision(),
        sources: inputs,
        comp: comp.clone(),
    };
    let ui = ui.clone();
    std::thread::Builder::new()
        .name("camera-tracker".into())
        .spawn(move || {
            bae_gpu::warm_up::lower_priority();
            let run = || {
                let stage = read().map_err(|e| e.to_string())?;
                let info = query::comp_info(&stage, &comp).ok_or("Missing composition")?;
                let duration = info.frame_duration().0.max(1);
                let timing = query::layer_timing(&stage, &source);
                let lo = timing.in_point.min(timing.out_point).0.max(0);
                let hi = timing.in_point.max(timing.out_point).min(info.duration).0;
                let mut frames = Vec::new();
                let mut size = [0, 0];
                {
                    let mut slot = ready.renderer.lock().map_err(|e| e.to_string())?;
                    let renderer = &mut *slot;
                    if let Some(src) = sources {
                        for entry in src.0.snapshot().iter() {
                            renderer.providers_mut().register(entry.item().clone());
                        }
                    }
                    if let Some(fx) = effects {
                        renderer.sync_effects(&fx.0);
                    }
                    let mut picture = pictures(
                        renderer,
                        FrameInput {
                            stage: &stage,
                            comp: &comp,
                            source: &source,
                            duration,
                            fields: false,
                            stop: &stop,
                        },
                    );
                    let mut previous = None;
                    let mut ids = Vec::new();
                    for (i, t) in ((lo + duration - 1) / duration * duration..hi)
                        .step_by(duration as usize)
                        .enumerate()
                    {
                        if stop.load(Ordering::Relaxed) {
                            return Err("Tracking stopped".into());
                        }
                        let image = picture(Time(t))?;
                        let current = image.texture();
                        let dimensions = [current.width(), current.height()];
                        if i > 0 && size != dimensions {
                            return Err("Track points changed size during analysis".into());
                        }
                        size = dimensions;
                        let measured = ready.features.measure(
                            &ready.gpu,
                            previous
                                .as_ref()
                                .map_or(current, super::worker::Image::texture),
                            current,
                            i as u32,
                            settings.detailed,
                            &mut ids,
                        )?;
                        frames.push((Time(t), measured));
                        previous = Some(image);
                    }
                }
                step.store(2, Ordering::Relaxed);
                ui.request_repaint();
                camera_track::solve(size, frames, settings, || stop.load(Ordering::Relaxed))
            };
            let _ = send.send(run());
            ui.request_repaint();
        })
        .map_err(|e| e.to_string())?;
    Ok(Some(job))
}
fn shared(r: &Resources) -> Shared {
    r.get::<Shared>().expect("camera tracker state").clone()
}
fn label(step: u8) -> &'static str {
    if step == 1 {
        "Analyzing in background (step 1 of 2)"
    } else {
        "Solving camera (step 2 of 2)"
    }
}
pub(crate) fn controls(ui: &mut egui::Ui, cx: &PanelContext<'_>, effect: &PrimPath) {
    let shared = shared(cx.resources);
    let mut state = shared.lock().unwrap();
    let settings = Settings::read(cx.stage, effect);
    if let Some(job) = state.jobs.get(effect) {
        ui.horizontal(|ui| {
            ui.label(tr(label(job.step.load(Ordering::Relaxed))));
            if ui.button(tr("Cancel")).clicked() {
                job.stop.store(true, Ordering::Relaxed);
            }
        });
        return;
    }
    let stored = query::attr_spec(cx.stage, effect, camera_track::SOLUTION).is_some();
    let completed = state.finished.get(effect);
    let restart = completed.is_some_and(|(s, _)| *s != settings);
    let mut analyze = restart || (!stored && completed.is_none());
    if !analyze {
        ui.horizontal(|ui| {
            let status = completed.map_or("Camera solved", |(_, s)| s.as_str());
            ui.label(tr(status));
            analyze = ui.button(tr("Analyze Camera")).clicked();
        });
    }
    if analyze && query::locked_layer_of(cx.stage, effect).is_none() {
        match start(cx, ui.ctx(), effect, settings) {
            Ok(Some(job)) => {
                state.jobs.insert(effect.clone(), job);
            }
            Ok(None) => {
                ui.label(tr("Tracker is warming up"));
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
            Err(e) => {
                state.finished.insert(effect.clone(), (settings, e));
            }
        }
    }
}
struct Host;
impl bae_ui::DialogHost for Host {
    fn is_open(&self, _: &Resources) -> bool {
        false
    }
    fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>) {
        let shared = shared(cx.resources);
        let mut state = shared.lock().unwrap();
        state.finished.retain(|p, _| cx.stage.has_prim(p));
        let mut completed = Vec::new();
        state.jobs.retain(|p, j| {
            if !cx.stage.has_prim(p) {
                return false;
            }
            let invalid = cx.stage.changes_since(j.revision).is_none_or(|events| {
                events.flat_map(|e| &e.changes).any(|change| {
                    let path = change.path();
                    if p.is_prefix_of(path) {
                        return Settings::read(cx.stage, p) != j.settings;
                    }
                    let touches = |path: &PrimPath| {
                        path == &j.comp
                            || j.sources
                                .iter()
                                .any(|src| src.is_prefix_of(path) || path.is_prefix_of(src))
                    };
                    touches(path)
                        || matches!(change,bae_model::Change::PrimMoved{from,..} if touches(from))
                })
            });
            j.revision = cx.stage.revision();
            if invalid {
                completed.push((
                    p.clone(),
                    j.settings,
                    Err("Analysis stopped because the project changed".into()),
                ));
                return false;
            }
            let result = match j.result.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => return true,
                Err(_) => Err("Analysis interrupted".into()),
            };
            let result = if j.stop.load(Ordering::Relaxed) {
                Err("Tracking stopped".into())
            } else {
                result
            };
            completed.push((p.clone(), j.settings, result));
            false
        });
        for (p, settings, result) in completed {
            let message = match result {
                Ok(solution) => {
                    cx.invoke_with(
                        "camera_tracker.record",
                        serde_json::json!({"effect":p,"solution":solution}),
                    );
                    "Camera solved".into()
                }
                Err(e) => e,
            };
            state.finished.insert(p, (settings, message));
        }
        if !state.jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }
}
pub(crate) fn banner(ui: &egui::Ui, cx: &PanelContext<'_>, area: egui::Rect) {
    let Some(shared) = cx.resources.get::<Shared>() else {
        return;
    };
    let s = shared.lock().unwrap();
    let session = crate::common::session(cx.resources);
    let session = session.lock();
    let Some((p, j)) = s
        .jobs
        .iter()
        .find(|(p, _)| query::owning_comp(cx.stage, p) == session.active_comp)
    else {
        return;
    };
    if cx
        .stage
        .prim(p)
        .and_then(|v| query::static_value(&v, &bae_model::effect::param_attr(26)))
        == Some(Value::Bool(true))
    {
        return;
    }
    let rect = egui::Rect::from_min_max(area.min, egui::pos2(area.right(), area.top() + 24.));
    let painter = ui.painter_at(area);
    painter.rect_filled(rect, 0., egui::Color32::from_rgb(37, 74, 129));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        tr(label(j.step.load(Ordering::Relaxed))),
        egui::FontId::proportional(14.),
        egui::Color32::WHITE,
    );
}
