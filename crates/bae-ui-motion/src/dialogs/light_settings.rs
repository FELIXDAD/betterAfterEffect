//! Light Settings: AE's Layer > New > Light (Cmd/Ctrl+Alt+Shift+L) and the
//! Light Settings of a selected light (Layer Settings, Cmd/Ctrl+Shift+Y).
//!
//! Fields the light type does not use are greyed out (AE). New: OK runs
//! `layer.create` with the settings, which are remembered for the next light;
//! the default name follows the type ("Spot Light 1") until it is typed.
//! Edit: OK runs one `light.set_settings` with what changed, at the current
//! time.

use bae_model::{PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_ui::color_picker::{self, ColorEdit};
use bae_ui::dialog::{self, Outcome};
use bae_ui::theme::color;
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui, vec2};
use serde_json::json;

use super::SharedDialogs;
use super::camera_settings::changed;
use super::comp_settings::menu_field;
use super::fields;
use crate::session::SharedSession;

/// Light types in AE's menu order: model token and English name.
const TYPES: [(&str, &str); 5] = [
    ("parallel", "Parallel"),
    ("spot", "Spot"),
    ("point", "Point"),
    ("ambient", "Ambient"),
    ("environment", "Environment"),
];

/// Falloff types: model token and English name.
const FALLOFFS: [(&str, &str); 3] = [
    ("none", "None"),
    ("smooth", "Smooth"),
    ("inverse_square_clamped", "Inverse Square Clamped"),
];

/// The light's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct LightValues {
    pub name: String,
    pub light_type: String,
    pub color: [f64; 4],
    pub intensity: f64,
    pub cone_angle: f64,
    pub cone_feather: f64,
    pub falloff: String,
    pub radius: f64,
    pub falloff_distance: f64,
    pub casts_shadows: bool,
    pub shadow_darkness: f64,
    pub shadow_diffusion: f64,
}

impl Default for LightValues {
    /// AE's factory light.
    fn default() -> Self {
        LightValues {
            name: String::new(),
            light_type: "spot".into(),
            color: [1.0, 1.0, 1.0, 1.0],
            intensity: 100.0,
            cone_angle: 90.0,
            cone_feather: 50.0,
            falloff: "none".into(),
            radius: 500.0,
            falloff_distance: 500.0,
            casts_shadows: false,
            shadow_darkness: 100.0,
            shadow_diffusion: 0.0,
        }
    }
}

impl LightValues {
    fn settings(&self) -> serde_json::Value {
        json!({
            "light_type": self.light_type,
            "color": self.color,
            "intensity": self.intensity,
            "cone_angle": self.cone_angle,
            "cone_feather": self.cone_feather,
            "falloff": self.falloff,
            "radius": self.radius,
            "falloff_distance": self.falloff_distance,
            "casts_shadows": self.casts_shadows,
            "shadow_darkness": self.shadow_darkness,
            "shadow_diffusion": self.shadow_diffusion,
        })
    }
}

/// What New Light remembers (AE keeps the last-used values).
#[derive(Clone, Debug, Default)]
pub struct LightSettingsMemory {
    pub values: Option<LightValues>,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    New {
        folder: PrimPath,
    },
    Edit {
        layer: PrimPath,
        initial: Box<LightValues>,
        time: Time,
    },
}

/// The open Light Settings dialog.
#[derive(Clone, Debug)]
pub struct LightSettings {
    target: Target,
    pub values: LightValues,
    /// The comp uses the Advanced 3D renderer (Environment lights).
    advanced_3d: bool,
    name_edited: bool,
    name_focused: bool,
    preview: bool,
}

/// Default layer names per light type ("Spot Light 1").
const NAMES: [(&str, &str); 5] = [
    ("parallel", "Parallel Light"),
    ("spot", "Spot Light"),
    ("point", "Point Light"),
    ("ambient", "Ambient Light"),
    ("environment", "Environment Light"),
];

/// "<Type> Light N": the next free number among the comp's layers.
fn default_name(stage: &Stage, session: &SharedSession, folder: &PrimPath, ty: &str) -> String {
    let base = NAMES
        .iter()
        .find(|(t, _)| *t == ty)
        .map_or("Spot Light", |(_, n)| n);
    crate::common::numbered(stage, session, folder, base)
}

impl LightSettings {
    /// Layer > New > Light in the active comp.
    pub(crate) fn new_light(
        stage: &Stage,
        session: &SharedSession,
        memory: &LightSettingsMemory,
    ) -> Option<Self> {
        let comp = crate::common::active_comp(stage, session)?;
        let folder = query::layers_folder(&comp.path);
        let mut values = memory.values.clone().unwrap_or_default();
        values.name = default_name(stage, session, &folder, &values.light_type);
        Some(LightSettings {
            target: Target::New { folder },
            values,
            advanced_3d: renderer_is_advanced(stage, &comp.path),
            name_edited: false,
            name_focused: false,
            preview: true,
        })
    }

    /// Light Settings of a light layer, with its values at the current time.
    pub(crate) fn edit(stage: &Stage, session: &SharedSession, layer: &PrimPath) -> Option<Self> {
        if query::layer_kind(stage, layer).as_deref() != Some("light") {
            return None;
        }
        let comp = query::owning_comp(stage, layer)?;
        let time = session.lock().time(&comp);
        let at = query::to_key_time(stage, layer, time);
        let d = LightValues::default();
        let sample = |a: &str| query::sample_attr(stage, layer, a, at);
        let num = |a: &str, d: f64| sample(a).and_then(|v| v.as_f64()).unwrap_or(d);
        let token = |a: &str, d: &str| match sample(a) {
            Some(Value::Enum(t)) => t,
            _ => d.to_owned(),
        };
        let values = LightValues {
            name: query::display_name_of(stage, layer),
            light_type: token("light:type", "spot"),
            color: match sample("light:color") {
                Some(Value::Color(c)) => c,
                _ => d.color,
            },
            intensity: num("light:intensity", d.intensity),
            cone_angle: num("light:cone_angle", d.cone_angle),
            cone_feather: num("light:cone_feather", d.cone_feather),
            falloff: token("light:falloff", "none"),
            radius: num("light:radius", d.radius),
            falloff_distance: num("light:falloff_distance", d.falloff_distance),
            casts_shadows: sample("light:casts_shadows")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            shadow_darkness: num("light:shadow_darkness", d.shadow_darkness),
            shadow_diffusion: num("light:shadow_diffusion", d.shadow_diffusion),
        };
        Some(LightSettings {
            target: Target::Edit {
                layer: layer.clone(),
                initial: Box::new(values.clone()),
                time,
            },
            advanced_3d: renderer_is_advanced(stage, &comp),
            values,
            name_edited: true,
            name_focused: false,
            preview: true,
        })
    }

    pub fn is_new(&self) -> bool {
        matches!(self.target, Target::New { .. })
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        dialogs: &SharedDialogs,
    ) -> bool {
        let id = Id::new("bae-ui-motion.light-settings");
        let shown = dialog::window(ctx, id, tr("Light Settings"), |ui| {
            self.body(ui, id, cx.stage, session)
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

    fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession, dialogs: &SharedDialogs) {
        let v = &self.values;
        let name = v.name.trim().to_owned();
        match &self.target {
            Target::New { .. } => {
                crate::common::new_layer(
                    cx,
                    session,
                    bae_model::LayerKind::Light,
                    json!({ "name": name, "light": v.settings() }),
                );
                dialogs.lock().light.values = Some(v.clone());
            }
            Target::Edit {
                layer,
                initial,
                time,
            } => {
                let mut params = changed(&v.settings(), &initial.settings());
                if name != initial.name.trim() {
                    params["name"] = json!(name);
                }
                if params.as_object().is_some_and(|m| !m.is_empty()) {
                    params["layer"] = json!(layer);
                    params["time"] = json!(time);
                    cx.invoke_with("light.set_settings", params);
                }
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id, stage: &Stage, session: &SharedSession) -> Outcome {
        const LW: f32 = 120.0;
        ui.set_width(400.0);
        dialog::row(ui, LW, tr("Name:"), |ui| {
            let r = fields::name_field(
                ui,
                id.with("name"),
                &mut self.values.name,
                240.0,
                tr("Name"),
                &mut self.name_focused,
            );
            if r.changed() {
                self.name_edited = true;
            }
        });
        let mut retyped = false;
        dialog::group(ui, tr("Settings"), |ui| {
            ui.set_width(ui.available_width());
            let v = &mut self.values;
            dialog::row(ui, LW, tr("Light Type:"), |ui| {
                // Environment lights need the Advanced 3D renderer.
                let types: Vec<_> = TYPES
                    .iter()
                    .filter(|(t, _)| self.advanced_3d || *t != "environment")
                    .collect();
                let items: Vec<Option<String>> =
                    types.iter().map(|(_, n)| Some(tr(n).to_owned())).collect();
                let label = TYPES
                    .iter()
                    .find(|(t, _)| *t == v.light_type)
                    .map_or("Spot", |(_, n)| n);
                if let Some(i) = menu_field(
                    ui,
                    id.with("type"),
                    tr(label),
                    &items,
                    160.0,
                    tr("Light Type"),
                ) {
                    v.light_type = types[i].0.to_owned();
                    retyped = true;
                }
            });
            let ty = v.light_type.as_str();
            let spot = ty == "spot";
            let lit = matches!(ty, "parallel" | "spot" | "point");
            let shadows = ty != "ambient";
            dialog::row(ui, LW, tr("Color:"), |ui| {
                let title = tr("Light Color");
                if let ColorEdit::Commit(c) = color_picker::color_button(
                    ui,
                    "light-color",
                    &mut v.color,
                    vec2(36.0, 16.0),
                    title,
                    title,
                ) {
                    v.color = c;
                }
                if let Some(rgb) = color_picker::eyedropper_button(ui, id.with("dropper")) {
                    v.color = [rgb[0], rgb[1], rgb[2], 1.0];
                }
            });
            let number = |ui: &mut Ui, label: &str, value: &mut f64, range, unit: &str, name| {
                dialog::row(ui, LW, label, |ui| {
                    fields::number(ui, value, range, 0, unit, name);
                });
            };
            number(
                ui,
                tr("Intensity:"),
                &mut v.intensity,
                -1.0e5..=1.0e5,
                "%",
                tr("Intensity"),
            );
            ui.add_enabled_ui(spot, |ui| {
                number(
                    ui,
                    tr("Cone Angle:"),
                    &mut v.cone_angle,
                    0.0..=180.0,
                    "°",
                    tr("Cone Angle"),
                );
                number(
                    ui,
                    tr("Cone Feather:"),
                    &mut v.cone_feather,
                    0.0..=100.0,
                    "%",
                    tr("Cone Feather"),
                );
            });
            ui.add_enabled_ui(lit, |ui| {
                dialog::row(ui, LW, tr("Falloff:"), |ui| {
                    let items: Vec<Option<String>> = FALLOFFS
                        .iter()
                        .map(|(_, n)| Some(tr(n).to_owned()))
                        .collect();
                    let label = FALLOFFS
                        .iter()
                        .find(|(t, _)| *t == v.falloff)
                        .map_or("None", |(_, n)| n);
                    if let Some(i) = menu_field(
                        ui,
                        id.with("falloff"),
                        tr(label),
                        &items,
                        180.0,
                        tr("Falloff"),
                    ) {
                        v.falloff = FALLOFFS[i].0.to_owned();
                    }
                });
            });
            ui.add_enabled_ui(lit && v.falloff != "none", |ui| {
                number(
                    ui,
                    tr("Radius:"),
                    &mut v.radius,
                    0.0..=1.0e6,
                    tr("px"),
                    tr("Radius"),
                );
            });
            ui.add_enabled_ui(lit && v.falloff == "smooth", |ui| {
                number(
                    ui,
                    tr("Falloff Distance:"),
                    &mut v.falloff_distance,
                    0.0..=1.0e6,
                    tr("px"),
                    tr("Falloff Distance"),
                );
            });
            ui.add_enabled_ui(shadows, |ui| {
                bae_ui::widgets::checkbox(ui, &mut v.casts_shadows, tr("Casts Shadows"));
            });
            ui.add_enabled_ui(shadows && v.casts_shadows, |ui| {
                number(
                    ui,
                    tr("Shadow Darkness:"),
                    &mut v.shadow_darkness,
                    0.0..=100.0,
                    "%",
                    tr("Shadow Darkness"),
                );
                number(
                    ui,
                    tr("Shadow Diffusion:"),
                    &mut v.shadow_diffusion,
                    0.0..=1.0e5,
                    tr("px"),
                    tr("Shadow Diffusion"),
                );
            });
        });
        ui.label(
            egui::RichText::new(tr(
                "Note: Shadows are only cast from layers with 'Casts Shadows' enabled to layers with 'Accepts Shadows' enabled.",
            ))
            .color(color::TEXT_DIM),
        );
        // A new light's default name follows its type until it is typed.
        if retyped
            && !self.name_edited
            && let Target::New { folder } = &self.target
        {
            self.values.name = default_name(stage, session, folder, &self.values.light_type);
        }
        let mut out = Outcome::None;
        ui.horizontal(|ui| {
            // Live preview of edits is not implemented yet: shown like AE.
            ui.add_enabled_ui(false, |ui| {
                bae_ui::widgets::checkbox(ui, &mut self.preview, tr("Preview"));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                out = dialog::buttons(ui, !self.values.name.trim().is_empty());
            });
        });
        out
    }
}

/// Whether the comp renders with Advanced 3D.
fn renderer_is_advanced(stage: &Stage, comp: &PrimPath) -> bool {
    stage
        .prim(comp)
        .and_then(|v| query::static_value(&v, "renderer"))
        == Some(Value::Enum("advanced_3d".into()))
}

/// English strings of Light Settings (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Light Settings",
    "Light Settings...",
    "Light...",
    "Name:",
    "Settings",
    "Light Type:",
    "Light Type",
    "Parallel",
    "Spot",
    "Point",
    "Ambient",
    "Environment",
    "Parallel Light",
    "Spot Light",
    "Point Light",
    "Ambient Light",
    "Environment Light",
    "Color:",
    "Light Color",
    "Intensity:",
    "Intensity",
    "Cone Angle:",
    "Cone Angle",
    "Cone Feather:",
    "Cone Feather",
    "Falloff:",
    "Falloff",
    "None",
    "Smooth",
    "Inverse Square Clamped",
    "Radius:",
    "Radius",
    "Falloff Distance:",
    "Falloff Distance",
    "Casts Shadows",
    "Shadow Darkness:",
    "Shadow Darkness",
    "Shadow Diffusion:",
    "Shadow Diffusion",
    "Note: Shadows are only cast from layers with 'Casts Shadows' enabled to layers with 'Accepts Shadows' enabled.",
    "Preview",
];
