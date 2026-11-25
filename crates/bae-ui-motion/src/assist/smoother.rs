//! Window > Smoother: among three or more selected keyframes of a property, the
//! ones that stay within the tolerance of the line between the keyframes kept
//! around them are removed (first and last stay). One `key.delete`.

use bae_model::Stage;
use bae_motion::{KeyRef, query};
use bae_ui::theme::color;
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::Ui;
use serde_json::json;

use super::{comps, properties};
use crate::common;

/// The keyframes Smoother removes (Douglas–Peucker over time and value).
pub(crate) fn smooth_deletions(stage: &Stage, keys: &[KeyRef], tolerance: f64) -> Vec<KeyRef> {
    let mut out = Vec::new();
    for ((path, attr), times) in properties(keys).into_iter().filter(|(_, t)| t.len() >= 3) {
        let Some(spec) = query::attr_spec(stage, &path, &attr) else {
            continue;
        };
        let pts: Vec<(f64, Vec<f64>)> = times
            .iter()
            .filter_map(|t| Some((t.0 as f64, comps(&spec.keyframes.get(t)?.value)?)))
            .collect();
        if pts.len() != times.len() {
            continue;
        }
        let mut keep = vec![false; pts.len()];
        keep[0] = true;
        keep[pts.len() - 1] = true;
        let mut stack = vec![(0, pts.len() - 1)];
        while let Some((a, b)) = stack.pop() {
            let (mut worst, mut at) = (0.0, 0);
            for i in a + 1..b {
                let f = (pts[i].0 - pts[a].0) / (pts[b].0 - pts[a].0);
                let d = pts[i]
                    .1
                    .iter()
                    .zip(pts[a].1.iter().zip(&pts[b].1))
                    .map(|(v, (x, y))| (v - (x + (y - x) * f)).abs())
                    .fold(0.0, f64::max);
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
        out.extend(
            times
                .iter()
                .zip(&keep)
                .filter(|(_, k)| !**k)
                .map(|(t, _)| KeyRef::new(path.clone(), &attr, *t)),
        );
    }
    out
}

pub struct SmootherPanel {
    /// 0 Spatial Path, 1 Temporal Graph.
    apply_to: usize,
    tolerance: f64,
    message: Option<&'static str>,
}

impl Default for SmootherPanel {
    fn default() -> Self {
        SmootherPanel {
            apply_to: 1,
            tolerance: 1.0,
            message: None,
        }
    }
}

impl Panel for SmootherPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Smoother"))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        crate::actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        ui.horizontal(|ui| {
            ui.label(tr("Apply To:"));
            let items = [tr("Spatial Path"), tr("Temporal Graph")];
            widgets::dropdown(ui, ui.id().with("apply"), &mut self.apply_to, &items, 120.0);
        });
        ui.horizontal(|ui| {
            ui.label(tr("Tolerance:"));
            ui.add(
                egui::DragValue::new(&mut self.tolerance)
                    .range(0.0..=1.0e6)
                    .speed(0.1),
            );
        });
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(m) = self.message {
                ui.colored_label(color::TEXT_DIM, tr(m));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Apply")).clicked() {
                    let keys = common::session(cx.resources).lock().selected_keys.clone();
                    let enough = properties(&keys).iter().any(|(_, t)| t.len() >= 3);
                    self.message =
                        (!enough).then_some("Select at least three keyframes of a property.");
                    let gone = smooth_deletions(cx.stage, &keys, self.tolerance);
                    if !gone.is_empty() {
                        cx.invoke_with("key.delete", json!({ "keys": gone }));
                    }
                }
            });
        });
    }
}
