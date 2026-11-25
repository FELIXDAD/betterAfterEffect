//! AE's property value dialogs: Layer ▸ Transform ▸ Anchor Point…, Position…
//! (Cmd/Ctrl+Shift+P), Scale…, Orientation… (Cmd/Ctrl+Alt+Shift+R), Rotation…
//! (Cmd/Ctrl+Shift+R) and Opacity… (Cmd/Ctrl+Shift+O), and the keyframe menu's
//! Edit Value… for those properties. The dialog shows the first target's value
//! at the current time (or the keyframe's time); OK sets it on every target in
//! one undo step (`attr.set_many`: a keyframe where the property is animated).

use bae_anim::glam::DVec3;
use bae_model::{PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::comp_settings::menu_field;
use super::fields;

/// Which transform property the dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    AnchorPoint,
    Position,
    Scale,
    Orientation,
    Rotation,
    Opacity,
}

impl Kind {
    /// The kind for a Layer ▸ Transform item's `attr` parameter.
    pub fn from_param(attr: &str) -> Option<Kind> {
        Some(match attr {
            "anchor_point" => Kind::AnchorPoint,
            "position" => Kind::Position,
            "scale" => Kind::Scale,
            "orientation" => Kind::Orientation,
            "rotation" => Kind::Rotation,
            "opacity" => Kind::Opacity,
            _ => return None,
        })
    }

    /// The kind that edits a keyframe of `attr` (Edit Value…), and the rotation
    /// axis it is.
    pub fn of_attr(attr: &str) -> Option<Kind> {
        Some(match attr {
            "transform:anchor_point" => Kind::AnchorPoint,
            "transform:position" => Kind::Position,
            "transform:scale" => Kind::Scale,
            "transform:orientation" => Kind::Orientation,
            "transform:rotation" | "transform:rotation_x" | "transform:rotation_y" => {
                Kind::Rotation
            }
            "transform:opacity" => Kind::Opacity,
            _ => return None,
        })
    }

    fn title(self) -> &'static str {
        match self {
            Kind::AnchorPoint => "Anchor Point",
            Kind::Position => "Position",
            Kind::Scale => "Scale",
            Kind::Orientation => "Orientation",
            Kind::Rotation => "Rotation",
            Kind::Opacity => "Opacity",
        }
    }

    fn attr(self) -> &'static str {
        match self {
            Kind::AnchorPoint => "transform:anchor_point",
            Kind::Position => "transform:position",
            Kind::Scale => "transform:scale",
            Kind::Orientation => "transform:orientation",
            Kind::Rotation => "transform:rotation",
            Kind::Opacity => "transform:opacity",
        }
    }

    /// The Units menu (AE: Anchor Point in source terms, Position in comp
    /// terms, Scale in percent of the source or pixels).
    fn units(self) -> &'static [Unit] {
        match self {
            Kind::AnchorPoint => &[
                Unit::Pixels,
                Unit::Inches,
                Unit::Millimeters,
                Unit::OfSource,
            ],
            Kind::Position => &[Unit::Pixels, Unit::Inches, Unit::Millimeters, Unit::OfComp],
            Kind::Scale => &[Unit::OfSource, Unit::Pixels],
            _ => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Pixels,
    Inches,
    Millimeters,
    OfSource,
    OfComp,
}

impl Unit {
    fn name(self) -> &'static str {
        match self {
            Unit::Pixels => "pixels",
            Unit::Inches => "inches",
            Unit::Millimeters => "millimeters",
            Unit::OfSource => "% of source",
            Unit::OfComp => "% of composition",
        }
    }
}

/// Scale ▸ Preserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preserve {
    None,
    Xy,
    Xyz,
}

/// One layer the dialog sets, with the key time (layer time) to set it at.
#[derive(Clone, Debug)]
struct Target {
    layer: PrimPath,
    time: Time,
    /// Position in separate X / Y / Z properties.
    separate: bool,
}

/// The open value dialog.
#[derive(Clone, Debug)]
pub struct TransformValue {
    kind: Kind,
    targets: Vec<Target>,
    three_d: bool,
    /// Rotation attributes the rows edit (X, Y, Z for a 3D layer).
    axes: Vec<&'static str>,
    /// The values in pixels / percent / degrees (rotations: one per axis).
    values: [f64; 3],
    unit: Unit,
    preserve: Preserve,
    include_par: bool,
    /// Source size of the first target (pixels), comp size and pixel aspect.
    source: [f64; 2],
    comp: [f64; 2],
    par: f64,
}

impl TransformValue {
    /// The dialog for `layers` at comp time `now` (Layer ▸ Transform), or for
    /// one keyframe of `attr` (`layers` = its layer, `now` its comp time).
    pub(crate) fn new(
        stage: &Stage,
        comp: &PrimPath,
        layers: &[PrimPath],
        now: Time,
        kind: Kind,
        key_attr: Option<&str>,
    ) -> Option<Self> {
        let info = query::comp_info(stage, comp)?;
        let eval = bae_anim::Evaluator::new()
            .evaluate_comp(stage, comp, now)
            .ok()?;
        let first = eval.layers.iter().find(|l| layers.contains(&l.path))?;
        let three_d = first.switches.three_d;
        if kind == Kind::Orientation && !three_d {
            return None;
        }
        let t = first.transform;
        let size = crate::render::layer_bounds(first)
            .map_or([f64::from(info.width), f64::from(info.height)], |(a, b)| {
                [b.x - a.x, b.y - a.y]
            });
        let axes: Vec<&'static str> = match key_attr {
            Some("transform:rotation_x") => vec!["transform:rotation_x"],
            Some("transform:rotation_y") => vec!["transform:rotation_y"],
            Some(_) => vec!["transform:rotation"],
            None if three_d => vec![
                "transform:rotation_x",
                "transform:rotation_y",
                "transform:rotation",
            ],
            None => vec!["transform:rotation"],
        };
        let v3 = |v: DVec3| [v.x, v.y, v.z];
        let values = match kind {
            Kind::AnchorPoint => v3(t.anchor_point),
            Kind::Position => v3(t.position),
            Kind::Scale => v3(t.scale),
            Kind::Orientation => v3(t.orientation),
            Kind::Rotation => {
                let of = |a: &str| match a {
                    "transform:rotation_x" => t.rotation.x,
                    "transform:rotation_y" => t.rotation.y,
                    _ => t.rotation.z,
                };
                let mut r = [0.0; 3];
                for (i, a) in axes.iter().enumerate() {
                    r[i] = of(a);
                }
                r
            }
            Kind::Opacity => [t.opacity, 0.0, 0.0],
        };
        let targets = layers
            .iter()
            .filter(|l| !query::is_locked(stage, l))
            .map(|l| Target {
                layer: l.clone(),
                time: query::to_key_time(stage, l, now),
                separate: stage
                    .prim(l)
                    .and_then(|v| query::static_value(&v, "transform:separate_dimensions"))
                    .and_then(|v| v.as_bool())
                    == Some(true),
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return None;
        }
        Some(TransformValue {
            kind,
            targets,
            three_d,
            axes,
            values,
            unit: kind.units().first().copied().unwrap_or(Unit::Pixels),
            preserve: Preserve::None,
            include_par: false,
            source: size,
            comp: [f64::from(info.width), f64::from(info.height)],
            par: info.pixel_aspect,
        })
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.transform-value");
        let shown = dialog::window(ctx, id, tr(self.kind.title()), |ui| self.body(ui, id));
        match shown {
            Some(Outcome::Ok) => {
                let edits = self.edits(cx.stage);
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

    /// Pixels per unit on axis `i` (0 = x, 1 = y; z uses pixels).
    fn per_unit(&self, i: usize) -> f64 {
        match self.unit {
            Unit::Pixels => 1.0,
            Unit::Inches => 72.0,
            Unit::Millimeters => 72.0 / 25.4,
            Unit::OfSource if i < 2 => self.source[i].max(1e-9) / 100.0,
            Unit::OfComp if i < 2 => self.comp[i].max(1e-9) / 100.0,
            _ => 1.0,
        }
    }

    /// Scale in pixels: the source size times the scale (and the comp's pixel
    /// aspect ratio across with Include Pixel Aspect Ratio).
    fn scale_px(&self, i: usize) -> f64 {
        let par = if i == 0 && self.include_par {
            self.par
        } else {
            1.0
        };
        self.source.get(i).copied().unwrap_or(100.0) * par / 100.0
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_min_width(300.0);
        const LW: f32 = 70.0;
        match self.kind {
            Kind::AnchorPoint | Kind::Position => {
                let n = if self.three_d { 3 } else { 2 };
                for (i, label) in ["X:", "Y:", "Z:"].into_iter().enumerate().take(n) {
                    let k = self.per_unit(i);
                    let mut shown = self.values[i] / k;
                    dialog::row(ui, LW, tr(label), |ui| {
                        if fields::number(ui, &mut shown, -1e6..=1e6, 1, "", tr(&label[..1]))
                            .changed()
                        {
                            self.values[i] = shown * k;
                        }
                    });
                }
                self.units_row(ui, id, LW);
            }
            Kind::Scale => {
                let n = if self.three_d { 3 } else { 2 };
                let old = self.values;
                for (i, label) in ["Width:", "Height:", "Depth:"]
                    .into_iter()
                    .enumerate()
                    .take(n)
                {
                    let px = self.unit == Unit::Pixels && i < 2;
                    let k = if px { self.scale_px(i) } else { 1.0 };
                    let mut shown = self.values[i] * k;
                    let unit = if px { tr("px") } else { "%" };
                    dialog::row(ui, LW, tr(label), |ui| {
                        let name = label.trim_end_matches(':');
                        if fields::number(ui, &mut shown, -1e6..=1e6, 1, unit, tr(name)).changed() {
                            self.values[i] = shown / k.max(1e-9);
                        }
                    });
                }
                // Preserve the aspect ratio: the other values follow the one
                // that changed.
                let n_keep = match self.preserve {
                    Preserve::None => 0,
                    Preserve::Xy => 2,
                    Preserve::Xyz => 3,
                };
                if let Some(c) = (0..n_keep).find(|i| self.values[*i] != old[*i])
                    && old[c].abs() > 1e-9
                {
                    let f = self.values[c] / old[c];
                    for i in (0..n_keep).filter(|i| *i != c) {
                        self.values[i] = old[i] * f;
                    }
                }
                self.units_row(ui, id, LW);
                dialog::row(ui, LW, tr("Preserve:"), |ui| {
                    let all: &[Preserve] = if self.three_d {
                        &[Preserve::None, Preserve::Xy, Preserve::Xyz]
                    } else {
                        &[Preserve::None, Preserve::Xy]
                    };
                    let name = |p: Preserve| match p {
                        Preserve::None => tr("None"),
                        Preserve::Xy => tr("Current Aspect Ratio (xy)"),
                        Preserve::Xyz => tr("Current Aspect Ratio (xyz)"),
                    };
                    let items: Vec<Option<String>> =
                        all.iter().map(|p| Some(name(*p).to_owned())).collect();
                    if let Some(i) = menu_field(
                        ui,
                        id.with("preserve"),
                        name(self.preserve),
                        &items,
                        180.0,
                        tr("Preserve"),
                    ) {
                        self.preserve = all[i];
                    }
                });
                ui.add_enabled_ui(self.unit == Unit::Pixels, |ui| {
                    bae_ui::widgets::checkbox(
                        ui,
                        &mut self.include_par,
                        tr("Include Pixel Aspect Ratio"),
                    );
                });
            }
            Kind::Orientation => {
                for (i, label) in ["X:", "Y:", "Z:"].into_iter().enumerate() {
                    dialog::row(ui, LW, tr(label), |ui| {
                        fields::number(ui, &mut self.values[i], -1e6..=1e6, 1, "°", label);
                    });
                }
            }
            Kind::Rotation => {
                let names = ["X", "Y", "Z"];
                let several = self.axes.len() > 1;
                for (i, attr) in self.axes.clone().into_iter().enumerate() {
                    let axis = match attr {
                        "transform:rotation_x" => names[0],
                        "transform:rotation_y" => names[1],
                        _ => names[2],
                    };
                    // "1 x + 30.0°": whole turns and the rest, as AE shows it.
                    let v = self.values[i];
                    let mut turns = (v / 360.0).trunc();
                    let mut degrees = v - turns * 360.0;
                    ui.horizontal(|ui| {
                        if several {
                            ui.label(axis);
                        }
                        ui.label(tr("Revolutions:"));
                        let a =
                            fields::number(ui, &mut turns, -1e5..=1e5, 0, "", tr("Revolutions"));
                        ui.label("x");
                        ui.label(tr("+ Degrees:"));
                        let b = fields::number(ui, &mut degrees, -1e6..=1e6, 1, "°", tr("Degrees"));
                        if a.changed() || b.changed() {
                            self.values[i] = turns * 360.0 + degrees;
                        }
                    });
                }
                ui.label(
                    egui::RichText::new(tr("(positive is clockwise)"))
                        .color(bae_ui::theme::color::TEXT_DIM),
                );
            }
            Kind::Opacity => {
                dialog::row(ui, LW, tr("Opacity:"), |ui| {
                    fields::number(ui, &mut self.values[0], 0.0..=100.0, 0, "%", tr("Opacity"));
                });
            }
        }
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }

    fn units_row(&mut self, ui: &mut Ui, id: Id, lw: f32) {
        let all = self.kind.units();
        dialog::row(ui, lw, tr("Units:"), |ui| {
            let items: Vec<Option<String>> =
                all.iter().map(|u| Some(tr(u.name()).to_owned())).collect();
            if let Some(i) = menu_field(
                ui,
                id.with("units"),
                tr(self.unit.name()),
                &items,
                150.0,
                tr("Units"),
            ) {
                self.unit = all[i];
            }
        });
    }

    /// `attr.set_many` edits that set the value on every target.
    fn edits(&self, stage: &Stage) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        let v = self.values;
        for t in &self.targets {
            let mut push = |attr: &str, value: Value| {
                out.push(json!({"path": t.layer, "attr": attr, "time": t.time, "value": value}));
            };
            let current = |attr: &str| query::sample_attr(stage, &t.layer, attr, t.time);
            // A vector property in its own shape (2D layers keep their Z).
            let vector = |attr: &str, n: usize| match current(attr) {
                Some(Value::Vec2(_)) => Value::Vec2([v[0], v[1]]),
                Some(Value::Vec3(old)) => {
                    let z = if n == 3 { v[2] } else { old[2] };
                    Value::Vec3([v[0], v[1], z])
                }
                _ => Value::Vec3([v[0], v[1], v[2]]),
            };
            let n = if self.three_d { 3 } else { 2 };
            match self.kind {
                Kind::Position if t.separate => {
                    let attrs = [
                        "transform:position_x",
                        "transform:position_y",
                        "transform:position_z",
                    ];
                    for (i, a) in attrs.into_iter().enumerate().take(n) {
                        push(a, Value::Float(v[i]));
                    }
                }
                Kind::AnchorPoint | Kind::Position | Kind::Scale => {
                    push(self.kind.attr(), vector(self.kind.attr(), n));
                }
                Kind::Orientation => push(self.kind.attr(), Value::Vec3(v)),
                Kind::Rotation => {
                    for (i, a) in self.axes.iter().enumerate() {
                        push(a, Value::Float(v[i]));
                    }
                }
                Kind::Opacity => push(self.kind.attr(), Value::Float(v[0].clamp(0.0, 100.0))),
            }
        }
        out
    }
}

/// English strings of the dialogs (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Anchor Point",
    "Position",
    "Scale",
    "Orientation",
    "Rotation",
    "Opacity",
    "X:",
    "Y:",
    "Z:",
    "Units:",
    "Units",
    "pixels",
    "inches",
    "millimeters",
    "% of source",
    "% of composition",
    "Width:",
    "Height:",
    "Depth:",
    "Width",
    "Height",
    "Depth",
    "px",
    "Preserve:",
    "Preserve",
    "None",
    "Current Aspect Ratio (xy)",
    "Current Aspect Ratio (xyz)",
    "Include Pixel Aspect Ratio",
    "Revolutions:",
    "Revolutions",
    "+ Degrees:",
    "Degrees",
    "(positive is clockwise)",
    "Opacity:",
];
