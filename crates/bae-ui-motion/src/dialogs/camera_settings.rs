//! Camera Settings: AE's Layer > New > Camera (Cmd/Ctrl+Alt+Shift+C) and the
//! Camera Settings of a selected camera (Layer Settings, Cmd/Ctrl+Shift+Y).
//!
//! Only Zoom, Focus Distance and Aperture are stored (pixels); Preset, Angle of
//! View, Film Size, Focal Length and F-Stop are derived with AE's lens math
//! (`bae_motion::camera::lens`). New: OK runs `layer.create` with the settings;
//! edit: OK runs one `camera.set_settings` with what changed, at the current
//! time.

use bae_model::{PrimPath, Stage, Time};
use bae_motion::camera::lens::{self, Measure};
use bae_motion::query;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::SharedDialogs;
use super::comp_settings::menu_field;
use super::fields;
use crate::session::SharedSession;

/// Display units of Film Size and Focal Length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilmUnits {
    Pixels,
    Inches,
    #[default]
    Millimeters,
}

/// The stored camera settings.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraValues {
    pub name: String,
    /// one_node or two_node.
    pub camera_type: String,
    pub zoom: f64,
    pub depth_of_field: bool,
    pub focus_distance: f64,
    pub aperture: f64,
    pub blur_level: f64,
}

/// What New Camera remembers (AE keeps the last camera's lens and options).
#[derive(Clone, Debug, Default)]
pub struct CameraSettingsMemory {
    pub values: Option<CameraValues>,
    pub film_h: Option<f64>,
    pub lock_to_zoom: Option<bool>,
    pub units: FilmUnits,
    pub measure: Measure,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    New,
    Edit {
        layer: PrimPath,
        initial: Box<CameraValues>,
        /// Comp time of the edit (keyframed properties get a key there).
        time: Time,
    },
}

/// The open Camera Settings dialog.
#[derive(Clone, Debug)]
pub struct CameraSettings {
    target: Target,
    pub values: CameraValues,
    /// The comp's width and height (the lens math's dimensions).
    size: (u32, u32),
    /// Film size measured on the comp width, in mm (36 for the presets).
    film_h: f64,
    pub lock_to_zoom: bool,
    pub units: FilmUnits,
    pub measure: Measure,
    name_focused: bool,
    preview: bool,
}

impl CameraSettings {
    /// Layer > New > Camera in the active comp.
    pub(crate) fn new_camera(
        stage: &Stage,
        session: &SharedSession,
        memory: &CameraSettingsMemory,
    ) -> Option<Self> {
        let comp = crate::common::active_comp(stage, session)?;
        let size = (comp.width, comp.height);
        let w = f64::from(comp.width);
        let zoom = lens::zoom(50.0, lens::FILM_SIZE, w);
        let mut values = memory.values.clone().unwrap_or(CameraValues {
            name: String::new(),
            camera_type: "two_node".into(),
            zoom,
            depth_of_field: false,
            focus_distance: zoom,
            aperture: lens::DEFAULT_APERTURE,
            blur_level: 100.0,
        });
        let folder = query::layers_folder(&comp.path);
        values.name = crate::common::numbered(stage, session, &folder, "Camera");
        Some(CameraSettings {
            target: Target::New,
            values,
            size,
            film_h: memory.film_h.unwrap_or(lens::FILM_SIZE),
            lock_to_zoom: memory.lock_to_zoom.unwrap_or(true),
            units: memory.units,
            measure: memory.measure,
            name_focused: false,
            preview: true,
        })
    }

    /// Camera Settings of a camera layer, with its values at the current time.
    pub(crate) fn edit(stage: &Stage, session: &SharedSession, layer: &PrimPath) -> Option<Self> {
        if query::layer_kind(stage, layer).as_deref() != Some("camera") {
            return None;
        }
        let comp_path = query::owning_comp(stage, layer)?;
        let comp = query::comp_info(stage, &comp_path)?;
        let time = session.lock().time(&comp_path);
        let at = query::to_key_time(stage, layer, time);
        let num = |a: &str, d: f64| {
            query::sample_attr(stage, layer, a, at)
                .and_then(|v| v.as_f64())
                .unwrap_or(d)
        };
        let w = f64::from(comp.width);
        let zoom = num("camera:zoom", lens::zoom(50.0, lens::FILM_SIZE, w));
        let view = stage.prim(layer)?;
        let values = CameraValues {
            name: query::display_name(&view),
            camera_type: match query::static_value(&view, "camera:type") {
                Some(bae_model::Value::Enum(t)) => t,
                _ => "two_node".into(),
            },
            zoom,
            depth_of_field: query::sample_attr(stage, layer, "camera:depth_of_field", at)
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            focus_distance: num("camera:focus_distance", zoom),
            aperture: num("camera:aperture", lens::DEFAULT_APERTURE),
            blur_level: num("camera:blur_level", 100.0),
        };
        Some(CameraSettings {
            target: Target::Edit {
                layer: layer.clone(),
                initial: Box::new(values.clone()),
                time,
            },
            lock_to_zoom: (values.focus_distance - values.zoom).abs() < 1e-6,
            values,
            size: (comp.width, comp.height),
            film_h: lens::FILM_SIZE,
            units: FilmUnits::default(),
            measure: Measure::default(),
            name_focused: false,
            preview: true,
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
        let id = Id::new("bae-ui-motion.camera-settings");
        let shown = dialog::window(ctx, id, tr("Camera Settings"), |ui| self.body(ui, id));
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

    fn settings(v: &CameraValues) -> serde_json::Value {
        json!({
            "camera_type": v.camera_type,
            "zoom": v.zoom,
            "depth_of_field": v.depth_of_field,
            "focus_distance": v.focus_distance,
            "aperture": v.aperture,
            "blur_level": v.blur_level,
        })
    }

    fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession, dialogs: &SharedDialogs) {
        let v = &self.values;
        let name = v.name.trim().to_owned();
        match &self.target {
            Target::New => {
                crate::common::new_layer(
                    cx,
                    session,
                    bae_model::LayerKind::Camera,
                    json!({ "name": name, "camera": Self::settings(v) }),
                );
                let mut d = dialogs.lock();
                d.camera.values = Some(v.clone());
                d.camera.film_h = Some(self.film_h);
                d.camera.lock_to_zoom = Some(self.lock_to_zoom);
                d.camera.units = self.units;
                d.camera.measure = self.measure;
            }
            Target::Edit {
                layer,
                initial,
                time,
            } => {
                let mut params = changed(&Self::settings(v), &Self::settings(initial));
                if name != initial.name.trim() {
                    params["name"] = json!(name);
                }
                if params.as_object().is_some_and(|m| !m.is_empty()) {
                    params["layer"] = json!(layer);
                    params["time"] = json!(time);
                    cx.invoke_with("camera.set_settings", params);
                }
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        const LW: f32 = 112.0;
        ui.set_width(460.0);
        let (w, h) = self.size;
        let dim = lens::dim(w, h, self.measure);
        let wf = f64::from(w);
        let v = &mut self.values;
        dialog::row(ui, LW, tr("Type:"), |ui| {
            let types = [
                ("one_node", "One-Node Camera"),
                ("two_node", "Two-Node Camera"),
            ];
            let items: Vec<Option<String>> =
                types.iter().map(|(_, n)| Some(tr(n).to_owned())).collect();
            let current = types
                .iter()
                .position(|(t, _)| *t == v.camera_type)
                .unwrap_or(1);
            if let Some(i) = menu_field(
                ui,
                id.with("type"),
                tr(types[current].1),
                &items,
                180.0,
                tr("Type"),
            ) {
                v.camera_type = types[i].0.into();
            }
        });
        dialog::row(ui, LW, tr("Name:"), |ui| {
            fields::name_field(
                ui,
                id.with("name"),
                &mut v.name,
                240.0,
                tr("Name"),
                &mut self.name_focused,
            );
        });
        // Lens: the focal length and film size on the comp width.
        let focal = lens::focal(v.zoom, self.film_h, wf);
        let preset = (self.film_h == lens::FILM_SIZE)
            .then(|| {
                lens::PRESETS
                    .iter()
                    .position(|(_, f)| (focal - f).abs() < 0.005)
            })
            .flatten();
        dialog::row(ui, LW, tr("Preset:"), |ui| {
            let mut items: Vec<Option<String>> = vec![Some(tr("Custom").to_owned()), None];
            items.extend(lens::PRESETS.iter().map(|(n, _)| Some((*n).to_owned())));
            let label =
                preset.map_or_else(|| tr("Custom").to_owned(), |i| lens::PRESETS[i].0.into());
            if let Some(i) = menu_field(ui, id.with("preset"), &label, &items, 120.0, tr("Preset"))
                && i >= 2
            {
                // AE: a preset always goes back to 36 mm film.
                self.film_h = lens::FILM_SIZE;
                v.zoom = lens::zoom(lens::PRESETS[i - 2].1, lens::FILM_SIZE, wf);
                if self.lock_to_zoom {
                    v.focus_distance = v.zoom;
                }
            }
        });
        ui.add_space(4.0);
        dialog::row(ui, LW, tr("Zoom:"), |ui| {
            let mut z = v.zoom;
            if fields::number(ui, &mut z, 1.0..=1.0e6, 2, tr("pixels"), tr("Zoom")).changed() {
                v.zoom = z;
                if self.lock_to_zoom {
                    v.focus_distance = z;
                }
            }
            ui.add_space(16.0);
            ui.label(tr("Angle of View:"));
            let mut aov = lens::angle_of_view(v.zoom, dim);
            if fields::number(ui, &mut aov, 0.01..=179.99, 2, "°", tr("Angle of View")).changed() {
                v.zoom = lens::zoom_from_angle(aov, dim);
                if self.lock_to_zoom {
                    v.focus_distance = v.zoom;
                }
            }
        });
        ui.add_space(4.0);
        bae_ui::widgets::checkbox(ui, &mut v.depth_of_field, tr("Enable Depth of Field"));
        ui.add_enabled_ui(v.depth_of_field, |ui| {
            dialog::row(ui, LW, tr("Focus Distance:"), |ui| {
                ui.add_enabled_ui(!self.lock_to_zoom, |ui| {
                    let mut f = v.focus_distance;
                    if fields::number(
                        ui,
                        &mut f,
                        0.0..=1.0e6,
                        2,
                        tr("pixels"),
                        tr("Focus Distance"),
                    )
                    .changed()
                    {
                        v.focus_distance = f;
                    }
                });
                ui.add_space(16.0);
                if bae_ui::widgets::checkbox(ui, &mut self.lock_to_zoom, tr("Lock to Zoom"))
                    .changed()
                    && self.lock_to_zoom
                {
                    v.focus_distance = v.zoom;
                }
            });
            dialog::row(ui, LW, tr("Aperture:"), |ui| {
                let mut a = v.aperture;
                if fields::number(ui, &mut a, 0.0..=1.0e6, 2, tr("pixels"), tr("Aperture"))
                    .changed()
                {
                    v.aperture = a;
                }
            });
            dialog::row(ui, LW, tr("F-Stop:"), |ui| {
                let mut s = if v.aperture > 0.0 {
                    lens::f_stop(focal, v.aperture, self.film_h, wf)
                } else {
                    0.0
                };
                if fields::number(ui, &mut s, 0.01..=1.0e6, 2, "", tr("F-Stop")).changed() {
                    v.aperture = lens::aperture(focal, s, self.film_h, wf);
                }
            });
            dialog::row(ui, LW, tr("Blur Level:"), |ui| {
                let mut b = v.blur_level;
                if fields::number(ui, &mut b, 0.0..=10000.0, 0, "%", tr("Blur Level")).changed() {
                    v.blur_level = b;
                }
            });
        });
        ui.add_space(4.0);
        // Film size and focal length in the chosen units.
        let (scale, unit) = match self.units {
            FilmUnits::Millimeters => (1.0, tr("mm")),
            FilmUnits::Inches => (1.0 / 25.4, tr("in")),
            FilmUnits::Pixels => (wf / self.film_h, tr("pixels")),
        };
        let measured = |film_h: f64| film_h * dim / wf;
        dialog::row(ui, LW, tr("Film Size:"), |ui| {
            let mut f = measured(self.film_h) * scale;
            let editable = self.units != FilmUnits::Pixels;
            ui.add_enabled_ui(editable, |ui| {
                if fields::number(ui, &mut f, 0.01..=1.0e5, 2, unit, tr("Film Size")).changed() {
                    // The focal length stays; the zoom follows the film.
                    let film_h = (f / scale) * wf / dim;
                    v.zoom = lens::zoom(focal, film_h, wf);
                    self.film_h = film_h;
                    if self.lock_to_zoom {
                        v.focus_distance = v.zoom;
                    }
                }
            });
        });
        dialog::row(ui, LW, tr("Focal Length:"), |ui| {
            let mut f = focal * scale;
            if fields::number(ui, &mut f, 0.01..=1.0e6, 2, unit, tr("Focal Length")).changed() {
                v.zoom = lens::zoom(f / scale, self.film_h, wf);
                if self.lock_to_zoom {
                    v.focus_distance = v.zoom;
                }
            }
        });
        dialog::row(ui, LW, tr("Units:"), |ui| {
            let all = [
                (FilmUnits::Pixels, "pixels"),
                (FilmUnits::Inches, "inches"),
                (FilmUnits::Millimeters, "millimeters"),
            ];
            let items: Vec<Option<String>> =
                all.iter().map(|(_, n)| Some(tr(n).to_owned())).collect();
            let current = all.iter().position(|(u, _)| *u == self.units).unwrap_or(2);
            if let Some(i) = menu_field(
                ui,
                id.with("units"),
                tr(all[current].1),
                &items,
                140.0,
                tr("Units"),
            ) {
                self.units = all[i].0;
            }
        });
        dialog::row(ui, LW, tr("Measure Film Size:"), |ui| {
            let all = [
                (Measure::Horizontally, "Horizontally"),
                (Measure::Vertically, "Vertically"),
                (Measure::Diagonally, "Diagonally"),
            ];
            let items: Vec<Option<String>> =
                all.iter().map(|(_, n)| Some(tr(n).to_owned())).collect();
            let current = all
                .iter()
                .position(|(m, _)| *m == self.measure)
                .unwrap_or(0);
            // The zoom stays; film size and angle of view are re-expressed.
            if let Some(i) = menu_field(
                ui,
                id.with("measure"),
                tr(all[current].1),
                &items,
                140.0,
                tr("Measure Film Size"),
            ) {
                self.measure = all[i].0;
            }
        });
        let mut out = Outcome::None;
        ui.add_space(4.0);
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

/// The entries of `new` that differ from `old` (both JSON objects).
pub(crate) fn changed(new: &serde_json::Value, old: &serde_json::Value) -> serde_json::Value {
    let mut out = json!({});
    if let (Some(n), Some(o)) = (new.as_object(), old.as_object()) {
        for (k, v) in n {
            if o.get(k) != Some(v) {
                out[k] = v.clone();
            }
        }
    }
    out
}

/// English strings of Camera Settings (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Camera Settings",
    "Camera Settings...",
    "Camera...",
    "Type:",
    "Type",
    "One-Node Camera",
    "Two-Node Camera",
    "Name:",
    "Preset:",
    "Custom",
    "Zoom:",
    "Zoom",
    "pixels",
    "Angle of View:",
    "Angle of View",
    "Enable Depth of Field",
    "Focus Distance:",
    "Focus Distance",
    "Lock to Zoom",
    "Aperture:",
    "Aperture",
    "F-Stop:",
    "F-Stop",
    "Blur Level:",
    "Blur Level",
    "Film Size:",
    "Film Size",
    "Focal Length:",
    "Focal Length",
    "Units:",
    "inches",
    "millimeters",
    "Measure Film Size:",
    "Measure Film Size",
    "Horizontally",
    "Vertically",
    "Diagonally",
    "Preview",
];
