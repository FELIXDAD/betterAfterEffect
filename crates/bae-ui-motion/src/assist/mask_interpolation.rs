//! Window > Mask Interpolation (AE's Smart Mask Interpolation): select two or
//! more Mask Path keyframes, Apply adds keyframes between each pair at the
//! keyframe rate, the shapes blending from one to the next. When the shapes
//! differ in vertex count (or Add Mask Path Vertices is on), both are resampled
//! to the same vertices along their outlines first. One `attr.set_many`.

use bae_model::{BezierPath, PrimType, Stage, Time, Value};
use bae_motion::KeyRef;
use bae_motion::path as bez;
use bae_motion::query;
use bae_ui::theme::color;
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::Ui;
use serde_json::json;

use crate::common;

/// Keyframe Rate choices: Auto (the comp's frame rate), then per second.
const RATES: [Option<f64>; 6] = [
    None,
    Some(15.0),
    Some(24.0),
    Some(25.0),
    Some(30.0),
    Some(60.0),
];

/// How Add Mask Path Vertices counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum VertexUnit {
    PixelsBetween,
    Total,
    PercentOfOutline,
}

#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub rate: usize,
    pub fields: bool,
    pub linear: bool,
    pub bending: f64,
    pub quality: f64,
    pub add_vertices: bool,
    pub vertex_amount: f64,
    pub vertex_unit: VertexUnit,
    pub matching: usize,
    pub one_to_one: bool,
    pub first_match: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            rate: 0,
            fields: false,
            linear: false,
            bending: 50.0,
            quality: 50.0,
            add_vertices: true,
            vertex_amount: 10.0,
            vertex_unit: VertexUnit::PixelsBetween,
            matching: 0,
            one_to_one: false,
            first_match: true,
        }
    }
}

/// The `attr.set_many` edits for the selected keys, or AE's message when fewer
/// than two Mask Path keyframes of one mask are selected.
pub(crate) fn interpolation_edits(
    stage: &Stage,
    keys: &[KeyRef],
    s: &Settings,
    comp_fps: f64,
) -> Result<Vec<serde_json::Value>, &'static str> {
    let mut masks: Vec<(&bae_model::PrimPath, Vec<Time>)> = Vec::new();
    for k in keys.iter().filter(|k| {
        k.attr == "path" && stage.prim(&k.path).is_some_and(|v| v.is_a(&PrimType::MASK))
    }) {
        match masks.iter_mut().find(|(p, _)| *p == &k.path) {
            Some((_, t)) => t.push(k.time),
            None => masks.push((&k.path, vec![k.time])),
        }
    }
    let rate = RATES[s.rate].unwrap_or(comp_fps) * if s.fields { 2.0 } else { 1.0 };
    let step = Time::from_seconds(1.0 / rate.max(1.0));
    let mut edits = Vec::new();
    for (mask, mut times) in masks.into_iter().filter(|(_, t)| t.len() >= 2) {
        times.sort();
        let Some(spec) = query::attr_spec(stage, mask, "path") else {
            continue;
        };
        let shape = |t: &Time| match spec.keyframes.get(t).map(|k| &k.value) {
            Some(Value::Bezier(b)) => Some(b.clone()),
            _ => None,
        };
        for pair in times.windows(2) {
            let (Some(a), Some(b)) = (shape(&pair[0]), shape(&pair[1])) else {
                continue;
            };
            let (a, b, resampled) = match_vertices(&a, &b, s);
            let edit = |t: Time, v: BezierPath| json!({"path": mask, "attr": "path", "time": t, "value": Value::Bezier(v)});
            if resampled {
                edits.push(edit(pair[0], a.clone()));
                edits.push(edit(pair[1], b.clone()));
            }
            let span = (pair[1] - pair[0]).0 as f64;
            let mut t = pair[0] + step;
            while (pair[1] - t).0 as f64 > step.0 as f64 * 0.5 {
                let f = (t - pair[0]).0 as f64 / span;
                edits.push(edit(t, bez::blend(&a, &b, f)));
                t += step;
            }
        }
    }
    if edits.is_empty() {
        return Err(
            "Please select at least 2 consecutive mask path keyframes to interpolate between.",
        );
    }
    Ok(edits)
}

/// Both shapes with the same vertices (resampled along their outlines when the
/// counts differ or Add Mask Path Vertices asks for it); whether they changed.
fn match_vertices(a: &BezierPath, b: &BezierPath, s: &Settings) -> (BezierPath, BezierPath, bool) {
    let (na, nb) = (a.vertices.len(), b.vertices.len());
    if na == nb && !s.add_vertices {
        return (a.clone(), b.clone(), false);
    }
    let longest = bez::outline_length(a).max(bez::outline_length(b));
    let wanted = match s.vertex_unit {
        VertexUnit::PixelsBetween => (longest / s.vertex_amount.max(1.0)).ceil() as usize,
        VertexUnit::Total => s.vertex_amount as usize,
        VertexUnit::PercentOfOutline => (100.0 / s.vertex_amount.max(0.1)).ceil() as usize,
    };
    let count = if s.add_vertices { wanted } else { 0 }
        .max(na)
        .max(nb)
        .max(3);
    (bez::resample(a, count), bez::resample(b, count), true)
}

#[derive(Default)]
pub struct MaskInterpolationPanel {
    settings: Settings,
    message: Option<&'static str>,
}

impl Panel for MaskInterpolationPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Mask Interpolation"))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        crate::actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let s = &mut self.settings;
        ui.horizontal(|ui| {
            ui.label(tr("Keyframe Rate:"));
            let names: Vec<String> = RATES
                .iter()
                .map(|r| r.map_or_else(|| tr("Auto").to_owned(), |r| r.to_string()))
                .collect();
            let items: Vec<&str> = names.iter().map(String::as_str).collect();
            widgets::dropdown(ui, ui.id().with("rate"), &mut s.rate, &items, 60.0);
            ui.label(tr("per second"));
        });
        widgets::checkbox(ui, &mut s.fields, tr("Keyframe Fields (doubles rate)"));
        widgets::checkbox(
            ui,
            &mut s.linear,
            tr("Use \u{201c}Linear\u{201d} Vertex Paths"),
        );
        ui.separator();
        for (label, v) in [
            ("Bending Resistance:", &mut s.bending),
            ("Quality:", &mut s.quality),
        ] {
            ui.horizontal(|ui| {
                ui.label(tr(label));
                ui.add(egui::DragValue::new(v).range(0.0..=100.0));
            });
        }
        ui.separator();
        widgets::checkbox(ui, &mut s.add_vertices, tr("Add Mask Path Vertices"));
        ui.add_enabled_ui(s.add_vertices, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.add(egui::DragValue::new(&mut s.vertex_amount).range(1.0..=1000.0));
                let units = [
                    tr("Pixels Between Vertices"),
                    tr("Total Vertices"),
                    tr("Percentage of Outline"),
                ];
                let mut i = s.vertex_unit as usize;
                widgets::dropdown(ui, ui.id().with("unit"), &mut i, &units, 140.0);
                s.vertex_unit = [
                    VertexUnit::PixelsBetween,
                    VertexUnit::Total,
                    VertexUnit::PercentOfOutline,
                ][i];
            });
        });
        ui.horizontal(|ui| {
            ui.label(tr("Matching Method:"));
            let methods = [tr("Auto"), tr("Curve"), tr("Polyline")];
            widgets::dropdown(
                ui,
                ui.id().with("matching"),
                &mut s.matching,
                &methods,
                80.0,
            );
        });
        widgets::checkbox(ui, &mut s.one_to_one, tr("Use 1:1 Vertex Matches"));
        widgets::checkbox(ui, &mut s.first_match, tr("First Vertices Match"));
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(m) = self.message {
                ui.colored_label(color::TEXT_DIM, tr(m));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Apply")).clicked() {
                    let session = common::session(cx.resources);
                    let keys = session.lock().selected_keys.clone();
                    let fps = common::active_comp(cx.stage, &session)
                        .map_or(30.0, |c| c.frame_rate.as_f64());
                    match interpolation_edits(cx.stage, &keys, &self.settings, fps) {
                        Ok(edits) => {
                            self.message = None;
                            cx.invoke_with("attr.set_many", json!({ "edits": edits }));
                        }
                        Err(m) => self.message = Some(m),
                    }
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use bae_model::{AuthorId, CommandRegistry, PrimPath};

    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezierPath {
        BezierPath {
            vertices: vec![[x1, y0], [x1, y1], [x0, y1], [x0, y0]],
            in_tangents: vec![[0.0, 0.0]; 4],
            out_tangents: vec![[0.0, 0.0]; 4],
            closed: true,
            feather: Vec::new(),
        }
    }

    #[test]
    fn apply_adds_blended_keyframes_between_the_selected_pair() {
        let mut stage = Stage::new();
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let author = AuthorId::new("user:test").unwrap();
        let run = |stage: &mut Stage, id: &str, p: serde_json::Value| {
            reg.execute(stage, &author, id, p).unwrap().output
        };
        let path = |v: &serde_json::Value| PrimPath::new(v["path"].as_str().unwrap()).unwrap();
        let comp = path(&run(
            &mut stage,
            "comp.create",
            json!({"width": 640, "height": 360, "fps": 30}),
        ));
        let layer = path(&run(
            &mut stage,
            "layer.create",
            json!({"comp": comp, "kind": "solid"}),
        ));
        run(
            &mut stage,
            "mask.create",
            json!({"layer": layer, "kind": "rect", "position": [100.0, 100.0], "size": [100.0, 100.0]}),
        );
        let mask = layer.child("masks").unwrap();
        let mask = stage.children(&mask)[0].clone();
        run(
            &mut stage,
            "attr.set_animated",
            json!({"path": mask, "attr": "path", "animated": true, "time": 0}),
        );
        let one = Time::from_seconds(1.0);
        let wide = Value::Bezier(rect(0.0, 0.0, 300.0, 100.0));
        run(
            &mut stage,
            "attr.set_at",
            json!({"path": mask, "attr": "path", "time": one, "value": wide}),
        );
        let keys = [
            KeyRef::new(mask.clone(), "path", Time::ZERO),
            KeyRef::new(mask.clone(), "path", one),
        ];
        // Same vertex count, no added vertices: 29 keys at 30 fps, blended.
        let s = Settings {
            add_vertices: false,
            ..Settings::default()
        };
        let edits = interpolation_edits(&stage, &keys, &s, 30.0).unwrap();
        assert_eq!(edits.len(), 29);
        let mid: Value = serde_json::from_value(edits[14]["value"].clone()).unwrap();
        let Value::Bezier(mid) = mid else { panic!() };
        assert!(
            (mid.vertices[0][0] - 225.0).abs() < 1.0,
            "{:?}",
            mid.vertices
        );
        // Added vertices: both ends resampled (and rewritten) to the same count.
        let edits = interpolation_edits(&stage, &keys, &Settings::default(), 30.0).unwrap();
        assert_eq!(edits.len(), 31);
        let first: Value = serde_json::from_value(edits[0]["value"].clone()).unwrap();
        let Value::Bezier(first) = first else {
            panic!()
        };
        assert!(first.vertices.len() > 4);
        // Nothing selected: AE's message.
        assert!(interpolation_edits(&stage, &keys[..1], &s, 30.0).is_err());
    }
}
