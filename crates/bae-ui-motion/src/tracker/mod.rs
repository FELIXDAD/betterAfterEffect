//! Motion source selection, tracker controls and non-blocking analysis progress.
mod dialogs;
mod gpu;
pub(crate) mod overlay;
pub(crate) mod stabilizer;
#[cfg(test)]
#[path = "tests/analysis.rs"]
mod tests;
mod worker;

use crate::common;
use bae_app::{AppBuilder, Resources};
use bae_model::{PrimPath, Stage, Value};
use bae_motion::{
    query,
    tracker::{self as model, Frame},
};
use bae_ui::{Panel, PanelContext, PanelKind, tr};
use dialogs::Dialog;
use egui::Ui;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, atomic::Ordering};

#[derive(Default)]
struct State {
    source: Option<PrimPath>,
    current: Option<PrimPath>,
    dialog: Option<(PrimPath, Dialog)>,
    job: Option<worker::Job>,
    live: Vec<Frame>,
    message: String,
}
type Shared = Arc<Mutex<State>>;
fn shared(resources: &Resources) -> Shared {
    resources
        .get::<Shared>()
        .expect("motion tracker state")
        .clone()
}

struct TrackerPanel;
pub(crate) fn register(app: &mut AppBuilder) {
    app.insert_resource(stabilizer::Shared::default());
    bae_ui::i18n::register(&[
        ("Analyzing", "正在分析"),
        ("Analyzing in background", "正在后台分析"),
        ("Analysis cancelled", "分析已取消"),
        ("Analyze", "分析"),
    ]);
    app.insert_resource(Shared::default());
    let gpu = Arc::new(gpu::TrackingGpu::default());
    app.insert_resource(gpu.clone());
    let _ = app.register_arc::<dyn bae_gpu::WarmUp>("motion.tracker", gpu);
    let _ = app.register_arc::<dyn bae_ui::DialogHost>("tracker", Arc::new(Host));
    crate::register_panel(
        app,
        "tracker",
        PanelKind::new("Tracker", |_| Box::new(TrackerPanel)),
    );
    bae_ui::i18n::register(dialogs::ZH);
}
fn configure(cx: &mut PanelContext<'_>, tracker: &PrimPath, values: BTreeMap<String, Value>) {
    cx.invoke_with(
        "tracker.configure",
        json!({"tracker":tracker,"values":values}),
    );
}
fn set(cx: &mut PanelContext<'_>, tracker: &PrimPath, key: &str, v: Value) {
    configure(cx, tracker, BTreeMap::from([(key.into(), v)]));
}
fn token(stage: &Stage, path: &PrimPath, key: &str) -> String {
    model::setting(stage, path, key)
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn flag(stage: &Stage, path: &PrimPath, key: &str) -> bool {
    model::setting(stage, path, key) == Some(Value::Bool(true))
}
fn combo(ui: &mut Ui, label: &str, selected: &mut String, items: &[(&str, &str)]) -> bool {
    let before = selected.clone();
    egui::ComboBox::from_id_salt(label)
        .selected_text(
            items
                .iter()
                .find(|(k, _)| *k == *selected)
                .map_or(tr("None"), |(_, s)| tr(s)),
        )
        .show_ui(ui, |ui| {
            for (key, name) in items {
                ui.selectable_value(selected, (*key).to_owned(), tr(name));
            }
        });
    before != *selected
}
fn choose_path(
    ui: &mut Ui,
    stage: &Stage,
    label: &str,
    current: &mut Option<PrimPath>,
    paths: &[PrimPath],
) -> bool {
    let before = current.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(tr(label));
        let name = current
            .as_ref()
            .map(|p| query::display_name_of(stage, p))
            .unwrap_or_else(|| tr("None").into());
        egui::ComboBox::from_id_salt(label)
            .selected_text(name)
            .show_ui(ui, |ui| {
                for p in paths {
                    ui.selectable_value(current, Some(p.clone()), query::display_name_of(stage, p));
                }
            });
    });
    *current != before
}
fn source_layers(stage: &Stage, comp: &PrimPath) -> Vec<PrimPath> {
    query::comp_layers(stage, comp)
        .into_iter()
        .filter(|p| {
            matches!(
                query::layer_kind(stage, p).as_deref(),
                Some("footage" | "solid" | "shape" | "text" | "pre_comp")
            )
        })
        .collect()
}
pub(crate) fn create(cx: &mut PanelContext<'_>, layer: &PrimPath, stabilize: bool) {
    let session = common::session(cx.resources);
    let Some(comp) = query::owning_comp(cx.stage, layer) else {
        return;
    };
    let time = session.lock().time(&comp);
    let Some((min, max)) = bae_anim::evaluate_comp(cx.stage, &comp, time)
        .ok()
        .and_then(|c| crate::render::layer_bounds(c.layer(layer)?))
    else {
        return;
    };
    cx.invoke_with(
        "tracker.create",
        json!({"layer":layer,"center":((min+max)*0.5).to_array(),"stabilize":stabilize}),
    );
    let folder = layer.child("trackers").expect("folder");
    let pending = (1..).find_map(|i| {
        let p = folder.child(&format!("tracker_{i}")).ok()?;
        (!cx.stage.has_prim(&p)).then_some(p)
    });
    let shared = shared(cx.resources);
    let mut s = shared.lock().unwrap();
    s.current = pending;
    s.source = Some(layer.clone());
    s.message.clear();
    session.lock().layer_panel = Some(layer.clone());
    cx.open_panel("layer");
    cx.open_panel("tracker");
}
impl Panel for TrackerPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let info = common::active_comp(cx.stage, &session);
        let layers = info
            .as_ref()
            .map(|c| source_layers(cx.stage, &c.path))
            .unwrap_or_default();
        let shared = shared(cx.resources);
        let (mut src, mut tracker, busy) = {
            let s = shared.lock().unwrap();
            (s.source.clone(), s.current.clone(), s.job.is_some())
        };
        src = src.filter(|l| layers.contains(l)).or_else(|| {
            session
                .lock()
                .selected_layers
                .iter()
                .find(|l| layers.contains(l))
                .cloned()
        });
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled(false, egui::Button::new(tr("Track Camera")));
            if ui
                .add_enabled(
                    src.as_ref().is_some_and(|l| !query::is_locked(cx.stage, l)),
                    egui::Button::new(tr("Warp Stabilizer")),
                )
                .clicked()
                && let Some(layer) = &src
            {
                crate::effects::apply(
                    cx,
                    std::slice::from_ref(layer),
                    bae_model::effect::stabilizer::MATCH_NAME,
                );
                cx.open_panel("effect_controls");
            }
        });
        let created = ui
            .horizontal_wrapped(|ui| {
                for (label, stable) in [("Track Motion", false), ("Stabilize Motion", true)] {
                    if ui
                        .add_enabled(
                            !busy && src.as_ref().is_some_and(|l| !query::is_locked(cx.stage, l)),
                            egui::Button::new(tr(label)),
                        )
                        .clicked()
                        && let Some(l) = &src
                    {
                        create(cx, l, stable);
                        return true;
                    }
                }
                false
            })
            .inner;
        if created {
            return;
        }
        let changed = ui
            .add_enabled_ui(!busy, |ui| {
                choose_path(ui, cx.stage, "Motion Source:", &mut src, &layers)
            })
            .inner;
        if changed {
            tracker = None;
            session.lock().layer_panel = src.clone();
            cx.open_panel("layer");
        }
        let tracks = src
            .as_ref()
            .map(|p| model::trackers(cx.stage, p))
            .unwrap_or_default();
        tracker = tracker
            .filter(|p| tracks.contains(p))
            .or_else(|| tracks.last().cloned());
        ui.add_enabled_ui(!busy, |ui| {
            choose_path(ui, cx.stage, "Current Track:", &mut tracker, &tracks)
        });
        let mut s = shared.lock().unwrap();
        s.source = src.clone();
        s.current = tracker.clone();
        let has_track = tracker.is_some();
        let track = tracker.unwrap_or_else(PrimPath::root);
        let locked = !has_track || query::locked_layer_of(cx.stage, &track).is_some();
        ui.add_enabled_ui(!busy && !locked, |ui| {
            let mut mode = model::mode(cx.stage, &track);
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Track Type:"));
                if combo(
                    ui,
                    "track_type",
                    &mut mode,
                    &[
                        ("stabilize", "Stabilize"),
                        ("transform", "Transform"),
                        ("parallel", "Parallel Corner Pin"),
                        ("perspective", "Perspective Corner Pin"),
                    ],
                ) {
                    set(cx, &track, "track_type", Value::Enum(mode.clone()));
                }
            });
            let corner = matches!(mode.as_str(), "parallel" | "perspective");
            ui.add_enabled_ui(!corner, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (key, label) in [
                        ("position", "Position"),
                        ("rotation", "Rotation"),
                        ("scale", "Scale"),
                    ] {
                        let mut on = flag(cx.stage, &track, key);
                        if ui.checkbox(&mut on, tr(label)).changed() {
                            set(cx, &track, key, Value::Bool(on));
                        }
                    }
                });
            });
            let target = if mode == "stabilize" {
                src.clone()
            } else {
                model::setting(cx.stage, &track, "target").and_then(|v| v.as_prim_ref().cloned())
            };
            ui.label(format!(
                "{} {}",
                tr("Motion Target:"),
                target
                    .as_ref()
                    .map(|p| query::display_name_of(cx.stage, p))
                    .unwrap_or_else(|| tr("None").into())
            ));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(mode != "stabilize", egui::Button::new(tr("Edit Target...")))
                    .clicked()
                {
                    s.dialog = Some((track.clone(), Dialog::target(cx, &track)));
                }
                if ui.button(tr("Options...")).clicked() {
                    s.dialog = Some((track.clone(), Dialog::options(cx.stage, &track)));
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Analyze:"));
            if busy {
                if ui.button(tr("Stop")).clicked()
                    && let Some(j) = &s.job
                {
                    j.stop.store(true, Ordering::Relaxed);
                }
                return;
            }
            let ready = cx
                .resources
                .get::<Arc<bae_gpu::GpuContext>>()
                .is_some_and(|g| {
                    cx.resources
                        .get::<Arc<gpu::TrackingGpu>>()
                        .and_then(|cache| cache.ready(g))
                        .is_some()
                });
            for (glyph, label, dir, single) in [
                ("|◀", "Analyze One Frame Backward", -1, true),
                ("◀", "Analyze Backward", -1, false),
                ("▶", "Analyze Forward", 1, false),
                ("▶|", "Analyze One Frame Forward", 1, true),
            ] {
                let resp = ui
                    .add_enabled(ready && !locked, egui::Button::new(glyph))
                    .on_hover_text(tr(if ready {
                        label
                    } else {
                        "Tracker is warming up"
                    }));
                if resp.clicked() {
                    s.message.clear();
                    s.live.clear();
                    session.lock().stop();
                    session.lock().layer_panel = src.clone();
                    cx.open_panel("layer");
                    match worker::start(
                        cx,
                        ui.ctx(),
                        &track,
                        session.lock().active_time(),
                        dir,
                        single,
                    ) {
                        Ok(job) => s.job = Some(job),
                        Err(e) => s.message = e,
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!busy && !locked, egui::Button::new(tr("Reset")))
                .clicked()
            {
                cx.invoke_with("tracker.reset", json!({"tracker":track}));
            }
            let has_keys = model::points(cx.stage, &track)
                .first()
                .and_then(|p| query::attr_spec(cx.stage, p, "attach_point"))
                .is_some_and(|a| !a.keyframes.is_empty());
            if ui
                .add_enabled(!busy && !locked && has_keys, egui::Button::new(tr("Apply")))
                .clicked()
            {
                s.dialog = Some((track.clone(), Dialog::Apply("xy".into())));
            }
        });
        if !s.message.is_empty() {
            ui.label(tr(&s.message));
        }
    }
}
struct Host;
impl bae_ui::DialogHost for Host {
    fn is_open(&self, r: &Resources) -> bool {
        shared(r).lock().unwrap().dialog.is_some()
    }
    fn ui(&self, ctx: &egui::Context, cx: &mut PanelContext<'_>) {
        stabilizer::tick(ctx, cx);
        let shared = shared(cx.resources);
        let mut s = shared.lock().unwrap();
        if let Some(job) = s.job.take() {
            if cx.stage.revision() != job.revision {
                s.message = "Analysis stopped because the project changed".into();
                s.live.clear();
            } else {
                let mut end = None;
                loop {
                    use std::sync::mpsc::TryRecvError;
                    match job.updates.try_recv() {
                        Ok(worker::Update::Frame(f)) => s.live.push(f),
                        Ok(worker::Update::End(r)) => {
                            end = Some(r);
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            end = Some(Err("Analysis interrupted".into()));
                            break;
                        }
                    }
                }
                if let Some(f) = s.live.last()
                    && let Some(c) = query::owning_comp(cx.stage, &job.tracker)
                        .and_then(|p| query::comp_info(cx.stage, &p))
                {
                    common::session(cx.resources).lock().set_time(&c, f.time);
                }
                if let Some(result) = end {
                    if let Err(e) = result {
                        s.message = e;
                    }
                    if !s.live.is_empty() {
                        cx.invoke_with(
                            "tracker.record",
                            json!({"tracker":job.tracker,"frames":std::mem::take(&mut s.live)}),
                        );
                    }
                } else {
                    s.job = Some(job);
                }
            }
        }
        if let Some((track, mut dialog)) = s.dialog.take()
            && cx.stage.has_prim(&track)
            && dialog.show(ctx, cx, &track)
        {
            s.dialog = Some((track, dialog));
        }
    }
}
