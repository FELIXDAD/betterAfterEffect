use super::worker;
use bae_model::{
    Change, PrimPath, Stage, Value,
    effect::{
        param_attr,
        stabilizer::{MATCH_NAME, QUALITY},
    },
};
use bae_motion::{effect::effects_of, query};
use bae_ui::{PanelContext, tr};
use egui::{Align2, Color32, FontId, Painter, Rect, Ui};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, mpsc::TryRecvError},
    time::Duration,
};

#[derive(Default)]
pub(super) struct State {
    observed: Option<(PrimPath, u64)>,
    pending: VecDeque<PrimPath>,
    statuses: BTreeMap<PrimPath, String>,
    running: Option<Run>,
    selected_point: Option<(PrimPath, usize)>,
}
struct Run {
    job: worker::Job,
    quality: i64,
    total: i64,
    frames: Vec<bae_motion::tracker::Frame>,
}
pub(super) type Shared = Arc<Mutex<State>>;
fn setting(stage: &Stage, path: &PrimPath, id: u32) -> Option<Value> {
    stage.prim(path)?.value(&param_attr(id)).cloned()
}
fn flag(stage: &Stage, path: &PrimPath, id: u32, default: bool) -> bool {
    setting(stage, path, id)
        .and_then(|v| v.as_bool().or_else(|| v.as_f64().map(|n| n != 0.)))
        .unwrap_or(default)
}
fn quality(stage: &Stage, path: &PrimPath) -> i64 {
    i64::from(flag(stage, path, 36, false)) + 2 * i64::from(flag(stage, path, 308, true))
}
fn queue(s: &mut State, p: &PrimPath) {
    if !s.pending.contains(p) {
        s.pending.push_back(p.clone());
    }
    s.statuses.remove(p);
}
fn input_changed(
    change: &Change,
    effect: &PrimPath,
    layer: &PrimPath,
    source: Option<&PrimPath>,
) -> bool {
    let path = change.path();
    if path == effect {
        return matches!(change, Change::PrimCreated { .. })
            || matches!(change,Change::AttrChanged{attr,..} if attr=="param:0036" || attr=="param:0308");
    }
    if path == layer {
        return matches!(change,Change::AttrChanged{attr,..} if ["source","in_point","out_point","start_time","stretch"].contains(&attr.as_str()));
    }
    source.is_some_and(|src| src.is_prefix_of(path))
        || matches!(change,Change::Resynced{path:root} if root.is_prefix_of(layer))
}
pub(super) fn tick(ctx: &egui::Context, cx: &mut PanelContext<'_>) {
    let Some(shared) = cx.resources.get::<Shared>().cloned() else {
        return;
    };
    let session = crate::common::session(cx.resources);
    let Some(comp) = crate::common::active_comp(cx.stage, &session) else {
        return;
    };
    let mut s = shared.lock().unwrap();
    let revision = cx.stage.revision();
    if s.observed.as_ref() != Some(&(comp.path.clone(), revision)) {
        if let Some((_, r)) = &s.observed
            && cx.stage.changes_since(*r).is_none()
        {
            *s = State::default();
        }
        let _ = cx.stage.changes_since(revision);
        let previous = s.observed.replace((comp.path.clone(), revision));
        for layer in query::comp_layers(cx.stage, &comp.path) {
            let effects = effects_of(cx.stage, &layer);
            let source = cx
                .stage
                .prim(&layer)
                .and_then(|l| l.value("source"))
                .and_then(|v| v.as_prim_ref().cloned());
            for p in &effects {
                if cx
                    .stage
                    .prim(p)
                    .and_then(|e| e.value("match_name"))
                    .and_then(Value::as_str)
                    != Some(MATCH_NAME)
                {
                    continue;
                }
                let changed = previous
                    .as_ref()
                    .filter(|(c, _)| c == &comp.path)
                    .and_then(|(_, r)| cx.stage.changes_since(*r))
                    .is_some_and(|mut events| {
                        events.any(|event| {
                            event
                                .changes
                                .iter()
                                .any(|change| input_changed(change, p, &layer, source.as_ref()))
                        })
                    });
                if changed {
                    if s.running.as_ref().is_some_and(|r| &r.job.tracker == p) {
                        s.running.take();
                    }
                    queue(&mut s, p);
                } else if !s.statuses.contains_key(p)
                    && s.running.as_ref().is_none_or(|r| &r.job.tracker != p)
                    && setting(cx.stage, p, QUALITY)
                        .as_ref()
                        .and_then(Value::as_f64)
                        != Some(quality(cx.stage, p) as f64)
                {
                    queue(&mut s, p);
                }
            }
        }
    }
    if let Some(mut r) = s.running.take()
        && cx.stage.has_prim(&r.job.tracker)
    {
        loop {
            match r.job.updates.try_recv() {
                Ok(worker::Update::Frame(f)) => r.frames.push(f),
                Ok(worker::Update::End(Ok(()))) => {
                    cx.invoke_with("tracker.record_stabilization",serde_json::json!({"effect":r.job.tracker,"quality":r.quality,"frames":r.frames}));
                    s.statuses.insert(r.job.tracker.clone(), String::new());
                    break;
                }
                Ok(worker::Update::End(Err(e))) => {
                    s.statuses.insert(r.job.tracker.clone(), e);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    s.statuses
                        .insert(r.job.tracker.clone(), "Analysis interrupted".into());
                    break;
                }
                Err(TryRecvError::Empty) => {
                    s.running = Some(r);
                    break;
                }
            }
        }
    }
    if s.running.is_none()
        && let Some(p) = s.pending.pop_front()
        && let Some(layer) = bae_motion::effect::layer_of_effect(&p)
        && cx.stage.has_prim(&p)
        && !query::is_locked(cx.stage, &layer)
    {
        let quality = quality(cx.stage, &p);
        let timing = query::layer_timing(cx.stage, &layer);
        let Some(info) =
            query::owning_comp(cx.stage, &layer).and_then(|c| query::comp_info(cx.stage, &c))
        else {
            return;
        };
        let total = (timing.out_point.max(timing.in_point).min(info.duration).0
            - timing.in_point.min(timing.out_point).0.max(0))
        .max(1);
        match worker::launch(cx, ctx, &p, worker::Mode::Stabilize(quality)) {
            Ok(job) => {
                s.running = Some(Run {
                    job,
                    quality,
                    total: (total - 1) / info.frame_duration().0 + 1,
                    frames: vec![],
                })
            }
            Err(e) if e == "Tracker is warming up" => {
                s.pending.push_front(p);
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(e) => {
                s.statuses.insert(p, e);
            }
        }
    }
}
pub(crate) fn controls(ui: &mut Ui, cx: &PanelContext<'_>, p: &PrimPath) {
    let Some(shared) = cx.resources.get::<Shared>() else {
        return;
    };
    let mut s = shared.lock().unwrap();
    if let Some(r) = s.running.as_ref().filter(|r| r.job.tracker == *p) {
        ui.label(format!(
            "{} {}/{}",
            tr("Analyzing"),
            r.frames.len(),
            r.total
        ));
        if ui.button(tr("Cancel")).clicked() {
            s.running.take();
            s.statuses.insert(p.clone(), "Analysis cancelled".into());
        }
    } else {
        if ui.button(tr("Analyze")).clicked() {
            queue(&mut s, p);
            ui.ctx().request_repaint();
        }
        if let Some(msg) = s.statuses.get(p).filter(|s| !s.is_empty()) {
            ui.label(tr(msg));
        }
    }
}
pub(crate) fn banner(cx: &PanelContext<'_>, comp: &PrimPath, painter: &Painter, rect: Rect) {
    let Some(shared) = cx.resources.get::<Shared>() else {
        return;
    };
    let s = shared.lock().unwrap();
    if let Some(r) = &s.running
        && query::owning_comp(cx.stage, &r.job.tracker).as_ref() == Some(comp)
        && !flag(cx.stage, &r.job.tracker, 26, false)
    {
        let strip = Rect::from_min_size(rect.min, egui::vec2(rect.width(), 25.));
        painter.rect_filled(strip, 0., Color32::from_rgb(36, 78, 155));
        painter.text(
            strip.center(),
            Align2::CENTER_CENTER,
            format!(
                "{}  {}%",
                tr("Analyzing in background"),
                (100 * r.frames.len() as i64 / r.total).min(100)
            ),
            FontId::proportional(14.),
            Color32::WHITE,
        );
    }
}

pub(crate) fn delete_point(cx: &mut PanelContext<'_>) -> bool {
    use bae_model::effect::stabilizer::DELETED;
    let Some(shared) = cx.resources.get::<Shared>() else {
        return false;
    };
    let Some((p, i)) = shared.lock().unwrap().selected_point.take() else {
        return false;
    };
    if !flag(cx.stage, &p, 280, false) || !cx.stage.has_prim(&p) {
        return false;
    }
    let session = crate::common::session(cx.resources);
    let Some(layer) = bae_motion::effect::layer_of_effect(&p) else {
        return false;
    };
    if !session.lock().selected_layers.contains(&layer) {
        return false;
    }
    let t = query::to_key_time(cx.stage, &p, session.lock().active_time());
    let mut entries = match setting(cx.stage, &p, DELETED) {
        Some(Value::FloatList(v)) => v,
        _ => vec![],
    };
    entries.extend([
        i as f64,
        f64::from(flag(cx.stage, &p, 306, true)),
        t.0 as f64,
    ]);
    cx.invoke_with("attr.set_at",serde_json::json!({"path":p,"attr":param_attr(DELETED),"time":t,"value":Value::FloatList(entries)}));
    true
}
pub(crate) fn points(
    cx: &PanelContext<'_>,
    ui: &Ui,
    painter: &Painter,
    comp: &bae_anim::EvaluatedComp,
    view: &crate::render::View,
) {
    use bae_anim::glam::DVec2;
    use bae_model::effect::stabilizer::{CELLS, MOTION};
    for layer in comp.layers.iter().filter(|l| l.active && l.visible) {
        for fx in layer.effects.iter().filter(|e| {
            e.enabled
                && e.match_name == MATCH_NAME
                && e.param(280).and_then(Value::as_bool) == Some(true)
        }) {
            let Some(Value::FloatList(points)) = fx.param(MOTION) else {
                continue;
            };
            let Some((lo, hi)) = crate::render::layer_bounds(layer) else {
                continue;
            };
            let radius = (2. * fx.param(301).and_then(Value::as_f64).unwrap_or(100.) / 100.) as f32;
            for (i, offset) in points.as_chunks::<2>().0.iter().take(CELLS).enumerate() {
                if bae_model::effect::stabilizer::excluded(fx.param(305), i, layer.layer_time) {
                    continue;
                }
                let xy = lo
                    + (hi - lo) * DVec2::new((i % 3) as f64 + 0.5, (i / 3) as f64 + 0.5) / 3.
                    + DVec2::new(offset[0], offset[1]);
                let world = layer.world_matrix.transform_point3(xy.extend(0.));
                if let Some(p) = view.project(world, crate::render::is_3d(layer)) {
                    let rect = Rect::from_center_size(p, egui::Vec2::splat(2. * radius.max(4.)));
                    if let Some(shared) = cx.resources.get::<Shared>() {
                        let id = ui.id().with((&fx.path, i));
                        let mut state = shared.lock().unwrap();
                        if ui.interact(rect, id, egui::Sense::click()).clicked() {
                            state.selected_point = Some((fx.path.clone(), i));
                        }
                        let color = if state
                            .selected_point
                            .as_ref()
                            .is_some_and(|(path, cell)| path == &fx.path && *cell == i)
                        {
                            Color32::WHITE
                        } else {
                            Color32::LIGHT_BLUE
                        };
                        painter.circle_stroke(p, radius, egui::Stroke::new(1., color));
                    }
                }
            }
        }
    }
}
