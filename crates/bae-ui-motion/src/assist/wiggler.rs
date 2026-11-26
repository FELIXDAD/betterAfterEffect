//! Window > Wiggler: random deviation keyframes between the first and last
//! selected keyframes of a property, at the frequency, up to the magnitude.
//! One `attr.set_many`. Deterministic: the same selection wiggles the same way.

use bae_anim::Evaluator;
use bae_model::{Stage, Time};
use bae_motion::{KeyRef, query};
use bae_ui::theme::color;
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::Ui;
use serde_json::json;

use super::{comps, from_comps, properties};
use crate::common;

#[derive(Clone, Debug)]
pub(crate) struct Settings {
    /// 0 Spatial Path, 1 Temporal Graph.
    pub apply_to: usize,
    /// 0 Smooth, 1 Jagged.
    pub noise: usize,
    /// 0 One Dimension (`axis`), 1 All Independently, 2 All the Same.
    pub dimension: usize,
    pub axis: usize,
    pub frequency: f64,
    pub magnitude: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            apply_to: 1,
            noise: 0,
            dimension: 1,
            axis: 0,
            frequency: 5.0,
            magnitude: 1.0,
        }
    }
}

/// xorshift64*: a small deterministic generator in [-1, 1].
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (v >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// The wiggle keyframes for the selected keys.
pub(crate) fn wiggle_edits(
    stage: &Stage,
    eval: &mut Evaluator,
    keys: &[KeyRef],
    s: &Settings,
) -> Result<Vec<serde_json::Value>, &'static str> {
    let step = Time::from_seconds(1.0 / s.frequency.max(0.01));
    let mut edits = Vec::new();
    for ((path, attr), times) in properties(keys) {
        let (first, last) = (times[0], times[times.len() - 1]);
        let seed = path
            .as_str()
            .bytes()
            .chain(attr.bytes())
            .fold(0x9e37_79b9_7f4a_7c15_u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
            });
        let mut rng = Rng(seed | 1);
        let mut t = first + step;
        while t < last {
            let comp_t = query::to_comp_time(stage, &path, t);
            if let Some(base) = common::current_value(eval, stage, &path, &attr, comp_t)
                && let Some(mut c) = comps(&base)
            {
                let mut noise = || {
                    let n = rng.next();
                    if s.noise == 1 {
                        n + 0.5 * rng.next()
                    } else {
                        n
                    }
                };
                match s.dimension {
                    0 => {
                        if let Some(v) = c.get_mut(s.axis) {
                            *v += noise() * s.magnitude;
                        }
                    }
                    1 => c.iter_mut().for_each(|v| *v += noise() * s.magnitude),
                    _ => {
                        let n = noise() * s.magnitude;
                        c.iter_mut().for_each(|v| *v += n);
                    }
                }
                edits.push(
                    json!({"path": path, "attr": attr, "time": t, "value": from_comps(&base, &c)}),
                );
            }
            t += step;
        }
    }
    if edits.is_empty() {
        return Err("Select at least two keyframes of a property.");
    }
    Ok(edits)
}

#[derive(Default)]
pub struct WigglerPanel {
    settings: Settings,
    message: Option<&'static str>,
}

impl Panel for WigglerPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Wiggler"))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        crate::actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let s = &mut self.settings;
        let row = |ui: &mut Ui, label: &str, id: &str, sel: &mut usize, items: &[&str]| {
            ui.horizontal(|ui| {
                ui.label(tr(label));
                widgets::dropdown(ui, ui.id().with(id), sel, items, 140.0);
            });
        };
        row(
            ui,
            "Apply To:",
            "apply",
            &mut s.apply_to,
            &[tr("Spatial Path"), tr("Temporal Graph")],
        );
        row(
            ui,
            "Noise Type:",
            "noise",
            &mut s.noise,
            &[tr("Smooth"), tr("Jagged")],
        );
        row(
            ui,
            "Dimension:",
            "dim",
            &mut s.dimension,
            &[
                tr("One Dimension"),
                tr("All Dimensions Independently"),
                tr("All Dimensions the Same"),
            ],
        );
        if s.dimension == 0 {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                widgets::dropdown(
                    ui,
                    ui.id().with("axis"),
                    &mut s.axis,
                    &["X", "Y", "Z"],
                    60.0,
                );
            });
        }
        ui.horizontal(|ui| {
            ui.label(tr("Frequency:"));
            ui.add(
                egui::DragValue::new(&mut s.frequency)
                    .range(0.01..=1000.0)
                    .speed(0.1),
            );
            ui.label(tr("per second"));
        });
        ui.horizontal(|ui| {
            ui.label(tr("Magnitude:"));
            ui.add(egui::DragValue::new(&mut s.magnitude).speed(0.1));
        });
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(m) = self.message {
                ui.colored_label(color::TEXT_DIM, tr(m));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Apply")).clicked() {
                    let keys = common::session(cx.resources).lock().selected_keys.clone();
                    let mut eval = Evaluator::new();
                    match wiggle_edits(cx.stage, &mut eval, &keys, &self.settings) {
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
