//! Project Settings: AE's File ▸ Project Settings (Cmd/Ctrl+Alt+Shift+K) with
//! its tabs — Video Rendering and Effects, Time Display Style, Color, Audio,
//! Expressions. What the project stores (time display style, color depth,
//! working space, linearize, 1.0 gamma blending, expression engine) is
//! editable; OK runs one `project.set_settings`. AE's other settings are shown
//! at their values, greyed.

use bae_model::{PrimPath, Stage, Value};
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

const TABS: [&str; 5] = [
    "Video Rendering and Effects",
    "Time Display Style",
    "Color",
    "Audio",
    "Expressions",
];

const DEPTHS: [(&str, &str); 3] = [
    ("8", "8 bits per channel"),
    ("16", "16 bits per channel"),
    ("32", "32 bits per channel (float)"),
];

const SPACES: [&str; 7] = [
    "",
    "sRGB IEC61966-2.1",
    "HDTV (Rec. 709)",
    "Adobe RGB (1998)",
    "image P3",
    "ProPhoto RGB",
    "ACEScg ACES Working Space AMPAS S-2014-004",
];

const ENGINES: [(&str, &str); 2] = [
    ("extendscript", "Legacy ExtendScript"),
    ("javascript", "JavaScript"),
];

/// The open Project Settings dialog.
#[derive(Clone, Debug)]
pub struct ProjectSettings {
    tab: usize,
    frames: bool,
    depth: String,
    space: String,
    linearize: bool,
    gamma_1: bool,
    engine: String,
}

impl ProjectSettings {
    pub(crate) fn new(stage: &Stage) -> Self {
        let path = PrimPath::new(bae_motion::project_settings::PROJECT).ok();
        let view = path.as_ref().and_then(|p| stage.prim(p));
        let get = |attr: &str| {
            view.as_ref()
                .and_then(|v| bae_motion::query::static_value(v, attr))
        };
        let text = |attr: &str, default: &str| match get(attr) {
            Some(Value::Enum(s) | Value::String(s)) => s,
            _ => default.to_owned(),
        };
        let flag = |attr: &str| matches!(get(attr), Some(Value::Bool(true)));
        ProjectSettings {
            tab: 0,
            frames: text("time_display", "timecode") == "frames",
            depth: text("bit_depth", "16"),
            space: text("working_space", ""),
            linearize: flag("linearize"),
            gamma_1: flag("blend_colors_using_gamma_1"),
            engine: text("expression_engine", "javascript"),
        }
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.project-settings");
        let shown = dialog::window(ctx, id, tr("Project Settings"), |ui| self.body(ui, id));
        let outcome = match shown {
            Some(o) => o,
            None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
            None => Outcome::None,
        };
        match outcome {
            Outcome::None => true,
            Outcome::Cancel => false,
            Outcome::Ok => {
                cx.invoke_with(
                    "project.set_settings",
                    json!({
                        "time_display": if self.frames { "frames" } else { "timecode" },
                        "bit_depth": self.depth,
                        "working_space": self.space,
                        "linearize": self.linearize,
                        "blend_colors_using_gamma_1": self.gamma_1,
                        "expression_engine": self.engine,
                    }),
                );
                false
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            for (i, t) in TABS.iter().enumerate() {
                if ui.selectable_label(self.tab == i, tr(t)).clicked() {
                    self.tab = i;
                }
            }
        });
        ui.separator();
        let lw = 170.0;
        let greyed = |ui: &mut Ui, label: &str, value: &str| {
            dialog::row(ui, lw, tr(label), |ui| {
                ui.add_enabled_ui(false, |ui| {
                    let mut i = 0;
                    bae_ui::widgets::dropdown(ui, label, &mut i, &[tr(value)], 260.0);
                });
            });
        };
        egui::Frame::new().inner_margin(6).show(ui, |ui| {
            ui.set_min_height(300.0);
            match self.tab {
                0 => {
                    let gpu = if cfg!(target_os = "macos") {
                        "Mercury GPU Acceleration (Metal)"
                    } else {
                        "Mercury GPU Acceleration (Direct3D)"
                    };
                    greyed(ui, "Use:", gpu);
                }
                1 => {
                    if ui.radio(!self.frames, tr("Timecode")).clicked() {
                        self.frames = false;
                    }
                    ui.indent(id.with("tc"), |ui| {
                        greyed(ui, "Footage Start Time:", "Use Media Source");
                        greyed(ui, "Default Base:", "30");
                    });
                    if ui.radio(self.frames, tr("Frames")).clicked() {
                        self.frames = true;
                    }
                    ui.indent(id.with("frames"), |ui| {
                        ui.add_enabled_ui(false, |ui| {
                            let mut feet = false;
                            bae_ui::widgets::checkbox(ui, &mut feet, tr("Use Feet + Frames:"));
                        });
                        greyed(ui, "Frame Count:", "Timecode Conversion");
                    });
                }
                2 => {
                    greyed(ui, "Color Engine:", "Adobe color managed");
                    dialog::row(ui, lw, tr("Depth:"), |ui| {
                        let names: Vec<&str> = DEPTHS.iter().map(|(_, n)| tr(n)).collect();
                        let mut i = DEPTHS
                            .iter()
                            .position(|(v, _)| *v == self.depth)
                            .unwrap_or(1);
                        if bae_ui::widgets::dropdown(ui, "depth", &mut i, &names, 260.0).changed() {
                            self.depth = DEPTHS[i].0.to_owned();
                        }
                    });
                    dialog::row(ui, lw, tr("Working Color Space:"), |ui| {
                        let names: Vec<&str> = SPACES
                            .iter()
                            .map(|s| if s.is_empty() { tr("None") } else { s })
                            .collect();
                        let mut i = SPACES.iter().position(|s| *s == self.space).unwrap_or(0);
                        if bae_ui::widgets::dropdown(ui, "space", &mut i, &names, 260.0).changed() {
                            self.space = SPACES[i].to_owned();
                        }
                    });
                    ui.add_enabled_ui(!self.space.is_empty(), |ui| {
                        bae_ui::widgets::checkbox(
                            ui,
                            &mut self.linearize,
                            tr("Linearize Working Space"),
                        );
                    });
                    bae_ui::widgets::checkbox(
                        ui,
                        &mut self.gamma_1,
                        tr("Blend Colors Using 1.0 Gamma"),
                    );
                    ui.add_enabled_ui(false, |ui| {
                        let mut on = true;
                        bae_ui::widgets::checkbox(
                            ui,
                            &mut on,
                            tr("Compensate for Scene-referred Profiles"),
                        );
                    });
                }
                3 => greyed(ui, "Sample Rate:", "48.000 kHz"),
                _ => {
                    dialog::row(ui, lw, tr("Expressions Engine:"), |ui| {
                        let names: Vec<&str> = ENGINES.iter().map(|(_, n)| tr(n)).collect();
                        let mut i = ENGINES
                            .iter()
                            .position(|(v, _)| *v == self.engine)
                            .unwrap_or(1);
                        if bae_ui::widgets::dropdown(ui, "engine", &mut i, &names, 260.0).changed()
                        {
                            self.engine = ENGINES[i].0.to_owned();
                        }
                    });
                }
            }
        });
        dialog::buttons(ui, true)
    }
}

/// English strings of Project Settings (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Project Settings",
    "Video Rendering and Effects",
    "Time Display Style",
    "Color",
    "Audio",
    "Expressions",
    "Use:",
    "Timecode",
    "Frames",
    "Footage Start Time:",
    "Use Media Source",
    "Default Base:",
    "Use Feet + Frames:",
    "Frame Count:",
    "Timecode Conversion",
    "Color Engine:",
    "Depth:",
    "8 bits per channel",
    "16 bits per channel",
    "32 bits per channel (float)",
    "Working Color Space:",
    "Linearize Working Space",
    "Blend Colors Using 1.0 Gamma",
    "Compensate for Scene-referred Profiles",
    "Sample Rate:",
    "Expressions Engine:",
    "Legacy ExtendScript",
    "Mercury GPU Acceleration (Metal)",
    "Mercury GPU Acceleration (Direct3D)",
];
