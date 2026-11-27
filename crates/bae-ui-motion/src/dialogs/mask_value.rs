//! Layer ▸ Mask ▸ Mask Shape… (Cmd/Ctrl+Shift+M), Mask Feather…
//! (Cmd/Ctrl+Shift+F), Mask Opacity… and Mask Expansion…: AE's mask value
//! dialogs. They show the first target mask's value now; OK sets every target
//! (Mask Shape: the first) in one undo step, a keyframe where the property is
//! animated.

use bae_model::{BezierPath, PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::comp_settings::menu_field;
use super::fields;

/// Which mask property the dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Shape,
    ShapePath,
    Feather,
    Opacity,
    Expansion,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Kind::Shape => "Mask Shape",
            Kind::ShapePath => "Path",
            Kind::Feather => "Mask Feather",
            Kind::Opacity => "Mask Opacity",
            Kind::Expansion => "Mask Expansion",
        }
    }
}

/// Mask Shape ▸ Units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Pixels,
    Inches,
    Millimeters,
    OfSource,
}

const UNITS: [Unit; 4] = [
    Unit::Pixels,
    Unit::Inches,
    Unit::Millimeters,
    Unit::OfSource,
];

impl Unit {
    fn name(self) -> &'static str {
        match self {
            Unit::Pixels => "pixels",
            Unit::Inches => "inches",
            Unit::Millimeters => "millimeters",
            Unit::OfSource => "% of source",
        }
    }
}

/// The open mask value dialog.
#[derive(Clone, Debug)]
pub struct MaskValue {
    kind: Kind,
    /// Target masks and their key times.
    masks: Vec<(PrimPath, Time)>,
    /// Feather: horizontal, vertical; Opacity / Expansion: the value.
    values: [f64; 2],
    /// Mask Feather ▸ Lock: both directions together.
    lock: bool,
    /// Mask Shape: the bounding box (left, top, right, bottom) in pixels, the
    /// path it had, the layer's size for % of source.
    bbox: [f64; 4],
    path: Option<BezierPath>,
    source: [f64; 2],
    unit: Unit,
    reset: bool,
    ellipse: bool,
}

impl MaskValue {
    pub(crate) fn new(
        stage: &Stage,
        comp: &PrimPath,
        masks: &[PrimPath],
        now: Time,
        kind: Kind,
    ) -> Option<Self> {
        let first = masks.first()?;
        let masks: Vec<(PrimPath, Time)> = if matches!(kind, Kind::Shape | Kind::ShapePath) {
            vec![(first.clone(), query::to_key_time(stage, first, now))]
        } else {
            masks
                .iter()
                .map(|m| (m.clone(), query::to_key_time(stage, m, now)))
                .collect()
        };
        let sample = |attr: &str| query::sample_attr(stage, first, attr, masks[0].1);
        let num = |attr: &str, d: f64| sample(attr).and_then(|v| v.as_f64()).unwrap_or(d);
        let values = match kind {
            Kind::Feather => match sample("feather") {
                Some(Value::Vec2(v)) => v,
                _ => [0.0, 0.0],
            },
            Kind::Opacity => [num("opacity", 100.0), 0.0],
            Kind::Expansion => [num("expansion", 0.0), 0.0],
            Kind::Shape | Kind::ShapePath => [0.0, 0.0],
        };
        let path = match sample("path") {
            Some(Value::Bezier(p)) => Some(p),
            _ => None,
        };
        let layer = query::owning_layer(stage, first)?;
        let lb = crate::masks::layer_box(stage, comp, &layer, now);
        let bbox = path.as_ref().and_then(crate::masks::bounds).unwrap_or(lb);
        Some(MaskValue {
            kind,
            masks,
            values,
            lock: values[0] == values[1],
            bbox,
            path,
            source: [lb[2] - lb[0], lb[3] - lb[1]],
            unit: Unit::Pixels,
            reset: false,
            ellipse: false,
        })
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.mask-value");
        let shown = dialog::window(ctx, id, tr(self.kind.title()), |ui| self.body(ui, id));
        match shown {
            Some(Outcome::Ok) => {
                let edits = self.edits();
                if !edits.is_empty() {
                    cx.invoke_with("attr.set_many", json!({ "edits": edits }));
                }
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    /// Pixels per Mask Shape unit across (`x`) or down.
    fn per_unit(&self, x: bool) -> f64 {
        match self.unit {
            Unit::Pixels => 1.0,
            Unit::Inches => 72.0,
            Unit::Millimeters => 72.0 / 25.4,
            Unit::OfSource => {
                let s = if x { self.source[0] } else { self.source[1] };
                s.max(1e-9) / 100.0
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_min_width(300.0);
        const LW: f32 = 80.0;
        match self.kind {
            Kind::Feather => {
                let old = self.values;
                for (i, label) in ["Horizontal:", "Vertical:"].into_iter().enumerate() {
                    dialog::row(ui, LW, tr(label), |ui| {
                        let name = label.trim_end_matches(':');
                        fields::number(
                            ui,
                            &mut self.values[i],
                            0.0..=32000.0,
                            1,
                            tr("px"),
                            tr(name),
                        );
                    });
                }
                if self.lock {
                    if self.values[0] != old[0] {
                        self.values[1] = self.values[0];
                    } else if self.values[1] != old[1] {
                        self.values[0] = self.values[1];
                    }
                }
                bae_ui::widgets::checkbox(ui, &mut self.lock, tr("Lock"));
            }
            Kind::Opacity => {
                dialog::row(ui, LW, tr("Opacity:"), |ui| {
                    fields::number(ui, &mut self.values[0], 0.0..=100.0, 0, "%", tr("Opacity"));
                });
            }
            Kind::Expansion => {
                dialog::row(ui, LW, tr("Expansion:"), |ui| {
                    fields::number(
                        ui,
                        &mut self.values[0],
                        -32000.0..=32000.0,
                        1,
                        tr("px"),
                        tr("Expansion"),
                    );
                });
            }
            Kind::Shape | Kind::ShapePath => {
                dialog::group(ui, tr("Bounding Box"), |ui| {
                    // AE: Top and Bottom, Left and Right in pairs.
                    for (i, label, x) in [
                        (1, "Top:", false),
                        (0, "Left:", true),
                        (2, "Right:", true),
                        (3, "Bottom:", false),
                    ] {
                        let k = self.per_unit(x);
                        let mut shown = self.bbox[i] / k;
                        dialog::row(ui, LW, tr(label), |ui| {
                            let name = label.trim_end_matches(':');
                            if fields::number(ui, &mut shown, -1e6..=1e6, 1, "", tr(name)).changed()
                            {
                                self.bbox[i] = shown * k;
                            }
                        });
                    }
                    dialog::row(ui, LW, tr("Units:"), |ui| {
                        let items: Vec<Option<String>> = UNITS
                            .iter()
                            .map(|u| Some(tr(u.name()).to_owned()))
                            .collect();
                        if let Some(i) = menu_field(
                            ui,
                            id.with("units"),
                            tr(self.unit.name()),
                            &items,
                            150.0,
                            tr("Units"),
                        ) {
                            self.unit = UNITS[i];
                        }
                    });
                });
                dialog::group(ui, tr("Shape"), |ui| {
                    ui.horizontal(|ui| {
                        bae_ui::widgets::checkbox(ui, &mut self.reset, tr("Reset To:"));
                        ui.add_enabled_ui(self.reset, |ui| {
                            let items = [
                                Some(tr("Rectangle").to_owned()),
                                Some(tr("Ellipse").to_owned()),
                            ];
                            let current = if self.ellipse {
                                tr("Ellipse")
                            } else {
                                tr("Rectangle")
                            };
                            if let Some(i) = menu_field(
                                ui,
                                id.with("shape"),
                                current,
                                &items,
                                120.0,
                                tr("Shape"),
                            ) {
                                self.ellipse = i == 1;
                            }
                        });
                    });
                });
            }
        }
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }

    /// The new mask path: Reset To's rectangle / ellipse in the box, else the
    /// old path moved and scaled from its box into the new one.
    fn shape_path(&self) -> Option<BezierPath> {
        let [l, t, r, b] = self.bbox;
        if self.reset {
            let fit = [l.min(r), t.min(b), l.max(r), t.max(b)];
            return Some(if self.ellipse {
                crate::masks::ellipse_path(fit)
            } else {
                crate::masks::rect_path(fit)
            });
        }
        let mut p = self.path.clone()?;
        let [ol, ot, or, ob] = crate::masks::bounds(&p)?;
        let sx = if (or - ol).abs() > 1e-9 {
            (r - l) / (or - ol)
        } else {
            1.0
        };
        let sy = if (ob - ot).abs() > 1e-9 {
            (b - t) / (ob - ot)
        } else {
            1.0
        };
        for v in &mut p.vertices {
            *v = [l + (v[0] - ol) * sx, t + (v[1] - ot) * sy];
        }
        for tan in p.in_tangents.iter_mut().chain(p.out_tangents.iter_mut()) {
            *tan = [tan[0] * sx, tan[1] * sy];
        }
        Some(p)
    }

    fn edits(&self) -> Vec<serde_json::Value> {
        let (attr, value) = match self.kind {
            Kind::Feather => ("feather", Value::Vec2(self.values)),
            Kind::Opacity => ("opacity", Value::Float(self.values[0].clamp(0.0, 100.0))),
            Kind::Expansion => ("expansion", Value::Float(self.values[0])),
            Kind::Shape | Kind::ShapePath => match self.shape_path() {
                Some(p) => ("path", Value::Bezier(p)),
                None => return Vec::new(),
            },
        };
        self.masks
            .iter()
            .map(|(m, time)| json!({"path": m, "attr": attr, "time": time, "value": value}))
            .collect()
    }
}

/// English strings of the dialogs (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Mask Shape",
    "Mask Feather",
    "Mask Opacity",
    "Mask Expansion",
    "Horizontal:",
    "Vertical:",
    "Horizontal",
    "Vertical",
    "px",
    "Lock",
    "Opacity:",
    "Opacity",
    "Expansion:",
    "Expansion",
    "Bounding Box",
    "Top:",
    "Left:",
    "Right:",
    "Bottom:",
    "Top",
    "Left",
    "Right",
    "Bottom",
    "Units:",
    "Units",
    "pixels",
    "inches",
    "millimeters",
    "% of source",
    "Shape",
    "Reset To:",
    "Rectangle",
    "Ellipse",
];
