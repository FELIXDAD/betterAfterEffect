//! Motion Sketch capture (Window > Motion Sketch): once armed, a drag in the
//! Composition panel records the layer's position in real time (times the
//! capture speed) from the start of the work area; releasing, or reaching its
//! end, writes Position keyframes, thinned by the panel's Smoothing.

use bae_anim::glam::DVec2;
use bae_model::{PrimPath, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::PanelContext;
use egui::{Painter, Pos2, Response, Stroke, Ui};
use serde_json::json;

use crate::render::View;
use crate::session::SharedSession;

#[derive(Default)]
pub(super) struct SketchState {
    capture: Option<Capture>,
}

struct Capture {
    layer: PrimPath,
    speed: f64,
    smoothing: f64,
    start: Time,
    end: Time,
    /// Input time (seconds) of the press.
    t0: f64,
    samples: Vec<(Time, DVec2)>,
}

/// Indices of the samples kept within `tolerance` pixels (Douglas–Peucker).
pub(super) fn thin(samples: &[(Time, DVec2)], tolerance: f64) -> Vec<usize> {
    let n = samples.len();
    if n < 3 {
        return (0..n).collect();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (samples[a].1, samples[b].1);
        let (mut worst, mut at) = (0.0, 0);
        for (i, s) in samples.iter().enumerate().take(b).skip(a + 1) {
            let f = (s.0 - samples[a].0).0 as f64 / (samples[b].0 - samples[a].0).0.max(1) as f64;
            let d = s.1.distance(pa.lerp(pb, f));
            if d > worst {
                worst = d;
                at = i;
            }
        }
        if worst > tolerance {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    (0..n).filter(|i| keep[*i]).collect()
}

/// Runs the capture; true while it owns the pointer (the Selection tool waits).
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    state: &mut SketchState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    view: &View,
    painter: &Painter,
) -> bool {
    let (pressed, down, now) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.time,
        )
    });
    // (The lock is released before the body locks again.)
    let arm = if state.capture.is_none() && pressed && resp.hovered() {
        session.lock().motion_sketch.take()
    } else {
        None
    };
    if let Some(arm) = arm {
        session.lock().stop();
        state.capture = Some(Capture {
            layer: arm.layer,
            speed: arm.speed,
            smoothing: arm.smoothing,
            start: info.work_area.start,
            end: info.work_area.end,
            t0: now,
            samples: Vec::new(),
        });
    }
    let Some(c) = state.capture.as_mut() else {
        return false;
    };
    let t = c.start + Time::from_seconds((now - c.t0) * c.speed / 100.0);
    let t = t.min(c.end);
    if let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) {
        c.samples.push((t, view.to_comp(pos)));
    }
    session.lock().set_time(info, t);
    ui.ctx().request_repaint();
    let line: Vec<Pos2> = c.samples.iter().map(|s| view.to_screen(s.1)).collect();
    painter.add(egui::Shape::line(
        line,
        Stroke::new(1.0, bae_ui::theme::color::ACCENT),
    ));
    if down && t < c.end {
        return true;
    }
    let c = state.capture.take().expect("capturing");
    if c.samples.is_empty() {
        return true;
    }
    let stage = cx.stage;
    let attr = "transform:position";
    // The layer's Z stays (3D layers).
    let z = stage
        .prim(&c.layer)
        .and_then(|v| query::static_value(&v, attr))
        .and_then(|v| match v {
            Value::Vec3(p) => Some(p[2]),
            _ => None,
        })
        .unwrap_or(0.0);
    let edits: Vec<_> = thin(&c.samples, c.smoothing)
        .into_iter()
        .map(|i| {
            let (t, p) = c.samples[i];
            json!({"path": c.layer, "attr": attr, "time": query::to_key_time(stage, &c.layer, t), "value": Value::Vec3([p.x, p.y, z])})
        })
        .collect();
    cx.invoke_with(
        "attr.set_animated",
        json!({"path": c.layer, "attr": attr, "animated": true, "time": query::to_key_time(stage, &c.layer, c.samples[0].0)}),
    );
    cx.invoke_with("attr.set_many", json!({ "edits": edits }));
    true
}
