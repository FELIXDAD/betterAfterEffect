//! Solid Settings: AE's Layer > New > Solid (Cmd/Ctrl+Y) and Layer > Solid
//! Settings (Cmd/Ctrl+Shift+Y) dialog.
//!
//! New: OK runs `layer.create` (kind `solid`) with the name, size, pixel aspect
//! ratio and color; the color is remembered for the next new solid (AE). The
//! default name follows the color ("Dark Gray Solid 1") until the user types
//! one. Edit: OK runs one `solid.set_settings` with what changed (one undo
//! step); "Affect all layers that use this solid" decides whether a shared
//! solid changes for every layer or the layer gets its own copy.

use bae_model::{PrimPath, Stage};
use bae_motion::query;
use bae_motion::solid::{self, SolidInfo};
use bae_ui::color_picker::{self, ColorEdit};
use bae_ui::dialog::{self, Outcome};
use bae_ui::theme::color;
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui, vec2};
use serde_json::json;

use super::SharedDialogs;
use super::comp_settings::{frame_aspect_text, menu_field};
use super::fields::{self, PIXEL_ASPECTS};
use crate::session::SharedSession;

/// AE's initial solid color before one was used: 17% gray (45/255).
pub const DEFAULT_COLOR: [f64; 4] = [45.0 / 255.0, 45.0 / 255.0, 45.0 / 255.0, 1.0];

/// Size units of the dialog (the solid is always stored in pixels).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Units {
    #[default]
    Pixels,
    Inches,
    Millimeters,
    PercentOfComp,
}

impl Units {
    const ALL: [Units; 4] = [
        Units::Pixels,
        Units::Inches,
        Units::Millimeters,
        Units::PercentOfComp,
    ];

    fn name(self) -> &'static str {
        match self {
            Units::Pixels => "pixels",
            Units::Inches => "inches",
            Units::Millimeters => "millimeters",
            Units::PercentOfComp => "% of composition",
        }
    }

    /// Pixels per unit; `comp` is the comp's size along the axis (for %).
    /// Inches and millimeters use 72 pixels per inch, like AE.
    fn scale(self, comp: u32) -> f64 {
        match self {
            Units::Pixels => 1.0,
            Units::Inches => 72.0,
            Units::Millimeters => 72.0 / 25.4,
            Units::PercentOfComp => f64::from(comp.max(1)) / 100.0,
        }
    }

    fn decimals(self) -> usize {
        match self {
            Units::Pixels => 0,
            Units::Inches => 2,
            Units::Millimeters | Units::PercentOfComp => 1,
        }
    }
}

/// The settings the dialog edits.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidValues {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    pub color: [f64; 4],
}

impl From<SolidInfo> for SolidValues {
    fn from(i: SolidInfo) -> Self {
        SolidValues {
            name: i.name,
            width: i.width.max(1),
            height: i.height.max(1),
            pixel_aspect: i.pixel_aspect,
            color: i.color,
        }
    }
}

/// What New Solid starts from, kept across runs (AE's Last Solid Section):
/// the color and options, and the size and pixel aspect ratio of the last
/// solid together with its comp's, so a new solid in a comp of that size
/// gets the same size again.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SolidSettingsMemory {
    pub color: Option<[f64; 4]>,
    pub lock_aspect: bool,
    pub units: Units,
    /// Width, height and pixel aspect ratio of the last solid.
    pub size: Option<(u32, u32, f64)>,
    /// Its comp's width, height and pixel aspect ratio.
    pub comp: Option<(u32, u32, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    New,
    Edit {
        /// The layer or the solid item that was asked for.
        target: PrimPath,
        initial: Box<SolidValues>,
        /// Layers that use the solid.
        users: usize,
        /// Opened on a layer (the Affect all checkbox applies).
        layer: bool,
    },
}

/// The open Solid Settings dialog.
#[derive(Clone, Debug)]
pub struct SolidSettings {
    target: Target,
    pub values: SolidValues,
    /// The active comp's width, height and pixel aspect ratio (Make Comp Size, %).
    comp: Option<(u32, u32, f64)>,
    pub units: Units,
    pub lock_aspect: bool,
    aspect: f64,
    /// The user typed a name: it no longer follows the color.
    name_edited: bool,
    name_focused: bool,
    pub affect_all: bool,
    preview: bool,
}

/// The active comp's size, if any.
fn comp_size(stage: &Stage, session: &SharedSession) -> Option<(u32, u32, f64)> {
    let info = crate::common::active_comp(stage, session)?;
    Some((info.width, info.height, info.pixel_aspect))
}

impl SolidSettings {
    /// Layer > New > Solid in the active comp: comp size, the remembered color.
    pub(crate) fn new_solid(
        stage: &Stage,
        session: &SharedSession,
        memory: &SolidSettingsMemory,
    ) -> Option<Self> {
        let comp = comp_size(stage, session)?;
        let color = memory.color.unwrap_or(DEFAULT_COLOR);
        // The last solid's size when it was made in a comp of this size.
        let (width, height, pixel_aspect) = match (memory.size, memory.comp) {
            (Some(size), Some(c)) if c == comp => size,
            _ => comp,
        };
        let mut s = SolidSettings {
            target: Target::New,
            values: SolidValues {
                name: String::new(),
                width,
                height,
                pixel_aspect,
                color,
            },
            comp: Some(comp),
            units: memory.units,
            lock_aspect: memory.lock_aspect,
            aspect: f64::from(comp.0) / f64::from(comp.1.max(1)),
            name_edited: false,
            name_focused: false,
            affect_all: false,
            preview: false,
        };
        s.values.name = default_name(stage, color);
        Some(s)
    }

    /// Solid Settings of a layer that uses a solid, or of a solid item.
    pub(crate) fn edit(
        stage: &Stage,
        session: &SharedSession,
        memory: &SolidSettingsMemory,
        target: &PrimPath,
    ) -> Option<Self> {
        let (item, layer) = match solid::solid_info(stage, target) {
            Some(i) => (i.path, false),
            None => (solid::layer_solid(stage, target)?, true),
        };
        let values = SolidValues::from(solid::solid_info(stage, &item)?);
        let comp = comp_size(stage, session);
        let units = match memory.units {
            Units::PercentOfComp if comp.is_none() => Units::Pixels,
            u => u,
        };
        Some(SolidSettings {
            target: Target::Edit {
                target: target.clone(),
                initial: Box::new(values.clone()),
                users: solid::solid_users(stage, &item).len(),
                layer,
            },
            aspect: f64::from(values.width) / f64::from(values.height.max(1)),
            values,
            comp,
            units,
            lock_aspect: false,
            name_edited: true,
            name_focused: false,
            affect_all: false,
            preview: false,
        })
    }

    pub fn is_new(&self) -> bool {
        matches!(self.target, Target::New)
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        dialogs: &SharedDialogs,
    ) -> bool {
        let id = Id::new("bae-ui-motion.solid-settings");
        let shown = dialog::window(ctx, id, tr("Solid Settings"), |ui| {
            self.body(ui, id, cx.stage)
        });
        let outcome = match shown {
            Some(o) => o,
            None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
            None => Outcome::None,
        };
        match outcome {
            Outcome::None => true,
            Outcome::Cancel => false,
            Outcome::Ok => {
                self.commit(cx, session, dialogs);
                false
            }
        }
    }

    fn valid(&self) -> bool {
        !self.values.name.trim().is_empty()
    }

    fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession, dialogs: &SharedDialogs) {
        let v = &self.values;
        let name = v.name.trim().to_owned();
        match &self.target {
            Target::New => {
                crate::common::new_layer(
                    cx,
                    session,
                    bae_model::LayerKind::Solid,
                    json!({
                        "name": name,
                        "size": [v.width, v.height],
                        "pixel_aspect": v.pixel_aspect,
                        "color": v.color,
                    }),
                );
                let mut d = dialogs.lock();
                d.solid.color = Some(v.color);
                d.solid.lock_aspect = self.lock_aspect;
                d.solid.units = self.units;
                d.solid.size = Some((v.width, v.height, v.pixel_aspect));
                d.solid.comp = self.comp;
                super::memory::save(&d);
            }
            Target::Edit {
                target,
                initial,
                layer,
                ..
            } => {
                let mut params = json!({ "target": target });
                if name != initial.name.trim() {
                    params["name"] = json!(name);
                }
                if (v.width, v.height) != (initial.width, initial.height) {
                    params["width"] = json!(v.width);
                    params["height"] = json!(v.height);
                }
                if v.pixel_aspect != initial.pixel_aspect {
                    params["pixel_aspect"] = json!(v.pixel_aspect);
                }
                if v.color != initial.color {
                    params["color"] = json!(v.color);
                }
                if params.as_object().is_some_and(|m| m.len() > 1) {
                    if *layer {
                        params["affect_all"] = json!(self.affect_all);
                    }
                    cx.invoke_with("solid.set_settings", params);
                }
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id, stage: &Stage) -> Outcome {
        const LW: f32 = 112.0;
        ui.set_width(380.0);
        dialog::row(ui, 44.0, tr("Name:"), |ui| {
            let r = fields::name_field(
                ui,
                id.with("name"),
                &mut self.values.name,
                300.0,
                tr("Name"),
                &mut self.name_focused,
            );
            if r.changed() {
                self.name_edited = true;
            }
        });
        let comp = self.comp;
        dialog::group(ui, tr("Size"), |ui| {
            ui.set_width(ui.available_width());
            let v = &mut self.values;
            let units = self.units;
            let ratio = frame_aspect_text(v.width, v.height, 1.0);
            let (cw, ch) = comp.map_or((v.width, v.height), |c| (c.0, c.1));
            dialog::row(ui, LW, tr("Width:"), |ui| {
                if let Some(w) = unit_field(ui, v.width, units, cw, tr("Width")) {
                    v.width = w;
                    if self.lock_aspect {
                        v.height = ((f64::from(w) / self.aspect).round() as u32).clamp(1, 30000);
                    }
                }
                ui.add_space(12.0);
                let mut lock = self.lock_aspect;
                let label = tr("Lock Aspect Ratio to {ratio}").replace("{ratio}", &ratio);
                if bae_ui::widgets::checkbox(ui, &mut lock, &label).changed() {
                    self.lock_aspect = lock;
                    self.aspect = f64::from(v.width) / f64::from(v.height.max(1));
                }
            });
            dialog::row(ui, LW, tr("Height:"), |ui| {
                if let Some(h) = unit_field(ui, v.height, units, ch, tr("Height")) {
                    v.height = h;
                    if self.lock_aspect {
                        v.width = ((f64::from(h) * self.aspect).round() as u32).clamp(1, 30000);
                    }
                }
            });
            dialog::row(ui, LW, tr("Units:"), |ui| {
                let all: Vec<Units> = Units::ALL
                    .into_iter()
                    .filter(|u| comp.is_some() || *u != Units::PercentOfComp)
                    .collect();
                let items: Vec<Option<String>> =
                    all.iter().map(|u| Some(tr(u.name()).to_owned())).collect();
                if let Some(i) = menu_field(
                    ui,
                    id.with("units"),
                    tr(units.name()),
                    &items,
                    150.0,
                    tr("Units"),
                ) {
                    self.units = all[i];
                }
            });
            dialog::row(ui, LW, tr("Pixel Aspect Ratio:"), |ui| {
                let current = PIXEL_ASPECTS
                    .iter()
                    .position(|(_, a)| (a - v.pixel_aspect).abs() < 1e-3);
                let label = current.map_or_else(
                    || format!("{} ({:.2})", tr("Custom"), v.pixel_aspect),
                    |i| tr(PIXEL_ASPECTS[i].0).to_owned(),
                );
                let items: Vec<Option<String>> = PIXEL_ASPECTS
                    .iter()
                    .map(|(n, _)| Some(tr(n).to_owned()))
                    .collect();
                if let Some(i) = menu_field(
                    ui,
                    id.with("par"),
                    &label,
                    &items,
                    220.0,
                    tr("Pixel Aspect Ratio"),
                ) {
                    v.pixel_aspect = PIXEL_ASPECTS[i].1;
                }
            });
            ui.add_space(2.0);
            let dim = |s: String| egui::RichText::new(s).color(color::TEXT_DIM);
            if let Some((cw, ch, _)) = comp {
                let pct =
                    |a: u32, b: u32| format!("{:.1}", f64::from(a) / f64::from(b.max(1)) * 100.0);
                dialog::row(ui, LW, "", |ui| {
                    ui.label(dim(
                        tr("Width: {pct}% of comp").replace("{pct}", &pct(v.width, cw))
                    ));
                });
                dialog::row(ui, LW, "", |ui| {
                    ui.label(dim(
                        tr("Height: {pct}% of comp").replace("{pct}", &pct(v.height, ch))
                    ));
                });
            }
            dialog::row(ui, LW, "", |ui| {
                ui.label(dim(tr("Frame Aspect Ratio: {ratio}").replace(
                    "{ratio}",
                    &frame_aspect_text(v.width, v.height, v.pixel_aspect),
                )));
            });
            ui.add_space(2.0);
            ui.add_enabled_ui(comp.is_some(), |ui| {
                if ui.button(tr("Make Comp Size")).clicked()
                    && let Some((cw, ch, par)) = comp
                {
                    v.width = cw;
                    v.height = ch;
                    v.pixel_aspect = par;
                    self.aspect = f64::from(cw) / f64::from(ch.max(1));
                }
            });
        });
        let mut recolored = false;
        dialog::group(ui, tr("Color"), |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let title = tr("Solid Color");
                let v = &mut self.values;
                if let ColorEdit::Commit(c) = color_picker::color_button(
                    ui,
                    "solid-color",
                    &mut v.color,
                    vec2(48.0, 22.0),
                    title,
                    title,
                ) {
                    v.color = c;
                    recolored = true;
                }
                if let Some(rgb) = color_picker::eyedropper_button(ui, id.with("dropper")) {
                    v.color = [rgb[0], rgb[1], rgb[2], 1.0];
                    recolored = true;
                }
            });
        });
        // AE: a new solid's default name follows its color until it is edited.
        if recolored && !self.name_edited {
            self.values.name = default_name(stage, self.values.color);
        }
        // AE: one user: the checkbox is checked and greyed; a shared solid:
        // unchecked by default, and then the layer gets a new solid (OK reads
        // "New").
        let mut forks = false;
        if let Target::Edit { users, layer, .. } = &self.target {
            ui.add_space(4.0);
            let shared = *layer && *users > 1;
            if *layer {
                let mut checked = self.affect_all || !shared;
                ui.add_enabled_ui(shared, |ui| {
                    bae_ui::widgets::checkbox(
                        ui,
                        &mut checked,
                        tr("Affect all layers that use this solid"),
                    );
                });
                if shared {
                    self.affect_all = checked;
                }
            }
            forks = shared && !self.affect_all;
            let text = if forks {
                tr("A new solid will be created for this layer.").to_owned()
            } else if *users <= 1 {
                tr("This change will affect 1 layer.").to_owned()
            } else {
                tr("This change will affect {n} layers.").replace("{n}", &users.to_string())
            };
            ui.label(egui::RichText::new(text).color(color::TEXT_DIM));
        }
        let mut out = Outcome::None;
        ui.horizontal(|ui| {
            // Live preview is not implemented yet: shown like AE, off.
            ui.add_enabled_ui(false, |ui| {
                bae_ui::widgets::checkbox(ui, &mut self.preview, tr("Preview"));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = if forks { tr("New") } else { tr("OK") };
                out = dialog::buttons_labeled(ui, self.valid(), ok, tr("Cancel"));
            });
        });
        out
    }
}

/// A size field in `units`; returns the new size in pixels when it changed.
fn unit_field(ui: &mut Ui, px: u32, units: Units, comp: u32, label: &str) -> Option<u32> {
    let scale = units.scale(comp);
    let mut value = f64::from(px) / scale;
    let unit = match units {
        Units::Pixels => tr("px"),
        Units::Inches => tr("in"),
        Units::Millimeters => tr("mm"),
        Units::PercentOfComp => "%",
    };
    let range = 1.0 / scale..=30000.0 / scale;
    fields::number(ui, &mut value, range, units.decimals(), unit, label)
        .changed()
        .then(|| ((value * scale).round() as u32).clamp(1, 30000))
}

/// AE-style color name used in a new solid's default name: "Black", "White",
/// grays ("Dark Gray"), tinted grays ("Medium Gray-Red") and hues with a
/// shade ("Dark Blue", "Light Green"). Returns English text to translate.
pub(crate) fn color_name(rgb: [f64; 3]) -> String {
    let [h, s, b] = color_picker::rgb_to_hsb(rgb);
    if b < 0.08 {
        return tr("Black").to_owned();
    }
    if s < 0.08 && b > 0.95 {
        return tr("White").to_owned();
    }
    let gray = if b < 0.4 {
        "Dark Gray"
    } else if b < 0.7 {
        "Medium Gray"
    } else {
        "Light Gray"
    };
    if s < 0.08 {
        return tr(gray).to_owned();
    }
    const HUES: [&str; 12] = [
        "Red",
        "Orange",
        "Yellow",
        "Yellow-Green",
        "Green",
        "Green-Cyan",
        "Cyan",
        "Sky Blue",
        "Blue",
        "Violet",
        "Magenta",
        "Pink",
    ];
    let hue = tr(HUES[(((h + 15.0) / 30.0) as usize) % 12]);
    if s < 0.35 {
        return tr("{gray}-{hue}")
            .replace("{gray}", tr(gray))
            .replace("{hue}", hue);
    }
    // Shades are whole templates: "Light" alone is the Light layer.
    let shade = if b < 0.5 {
        Some("Dark {hue}")
    } else if s < 0.6 && b > 0.8 {
        Some("Light {hue}")
    } else {
        None
    };
    match shade {
        Some(shade) => tr(shade).replace("{hue}", hue),
        None => hue.to_owned(),
    }
}

/// "<color> Solid <n>": the next free number among the solid items.
pub(crate) fn default_name(stage: &Stage, color: [f64; 4]) -> String {
    let base = tr("{color} Solid").replace("{color}", &color_name([color[0], color[1], color[2]]));
    let folder = PrimPath::new(query::SOLIDS_FOLDER).expect("valid");
    query::next_numbered_name(stage, &folder, &base)
}

/// English strings of Solid Settings (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Solid Settings",
    "Solid Settings...",
    "Solid...",
    "Layer Settings...",
    "Name:",
    "Name",
    "Size",
    "Width:",
    "Height:",
    "Lock Aspect Ratio to {ratio}",
    "Units:",
    "Units",
    "pixels",
    "inches",
    "millimeters",
    "% of composition",
    "in",
    "mm",
    "Pixel Aspect Ratio:",
    "Width: {pct}% of comp",
    "Height: {pct}% of comp",
    "Frame Aspect Ratio: {ratio}",
    "Make Comp Size",
    "Color",
    "Solid Color",
    "Affect all layers that use this solid",
    "This change will affect 1 layer.",
    "This change will affect {n} layers.",
    "A new solid will be created for this layer.",
    "New",
    "Preview",
    "{color} Solid",
    "Black",
    "White",
    "Dark Gray",
    "Medium Gray",
    "Light Gray",
    "{gray}-{hue}",
    "Dark {hue}",
    "Light {hue}",
    "Red",
    "Orange",
    "Yellow",
    "Yellow-Green",
    "Green",
    "Green-Cyan",
    "Cyan",
    "Sky Blue",
    "Blue",
    "Violet",
    "Magenta",
    "Pink",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_names() {
        let n = |r: u8, g: u8, b: u8| {
            color_name([
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            ])
        };
        assert_eq!(n(0, 0, 0), "Black");
        assert_eq!(n(255, 255, 255), "White");
        assert_eq!(n(45, 45, 45), "Dark Gray");
        assert_eq!(n(128, 128, 128), "Medium Gray");
        assert_eq!(n(200, 200, 200), "Light Gray");
        assert_eq!(n(255, 0, 0), "Red");
        assert_eq!(n(0, 0, 100), "Dark Blue");
        assert_eq!(n(150, 255, 150), "Light Green");
        assert_eq!(n(140, 120, 120), "Medium Gray-Red");
        assert_eq!(n(255, 0, 255), "Magenta");
    }

    #[test]
    fn units_convert_both_ways() {
        assert_eq!(Units::Inches.scale(0), 72.0);
        assert!((Units::Millimeters.scale(0) * 25.4 - 72.0).abs() < 1e-9);
        assert_eq!(Units::PercentOfComp.scale(1920), 19.2);
        // 50% of a 1920 comp is 960 px.
        assert_eq!(
            (50.0 * Units::PercentOfComp.scale(1920)).round() as u32,
            960
        );
    }
}
