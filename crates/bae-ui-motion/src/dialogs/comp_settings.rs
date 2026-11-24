//! Composition Settings: AE's New Composition (Cmd/Ctrl+N) and Composition
//! Settings (Cmd/Ctrl+K) dialog, with the Basic, Advanced and 3D Renderer tabs.
//!
//! New: OK runs `comp.create` with every setting and opens the comp; the
//! settings are remembered for the next new comp (AE). Edit: OK runs one
//! `comp.set_settings` with the settings that changed (one undo step; resizing
//! moves layers with the Advanced tab's anchor).

use bae_model::{PrimPath, Stage, Time};
use bae_motion::ResizeAnchor;
use bae_motion::query::{self, CompInfo};
use bae_ui::Accessible as _;
use bae_ui::color_picker::{self, ColorEdit};
use bae_ui::dialog::{self, Outcome};
use bae_ui::theme::color;
use bae_ui::{PanelContext, tr};
use egui::{Id, Sense, Ui, vec2};
use serde_json::json;

use super::SharedDialogs;
use super::fields::{self, PIXEL_ASPECTS};
use crate::session::{Pending, SharedSession};

/// A composition preset: English name, width, height, pixel aspect, fps.
type Preset = (&'static str, u32, u32, f64, f64);

/// AE 26's composition presets, in menu order (`None`: separator).
const PRESETS: &[Option<Preset>] = &[
    Some(("HD", 1920, 1080, 1.0, 24.0)),
    Some(("HD", 1920, 1080, 1.0, 25.0)),
    Some(("HD", 1920, 1080, 1.0, 29.97)),
    None,
    Some(("UHD (4K)", 3840, 2160, 1.0, 23.976)),
    Some(("UHD (4K)", 3840, 2160, 1.0, 25.0)),
    Some(("UHD (4K)", 3840, 2160, 1.0, 29.97)),
    Some(("UHD (8K)", 7680, 4320, 1.0, 23.976)),
    None,
    Some(("Social Media Portrait HD", 1080, 1920, 1.0, 30.0)),
    Some(("Social Media Landscape HD", 1920, 1080, 1.0, 30.0)),
    Some(("Social Media Portrait", 720, 1280, 1.0, 30.0)),
    Some(("Social Media Landscape", 1280, 720, 1.0, 30.0)),
    Some(("Social Media Square", 1080, 1080, 1.0, 30.0)),
    None,
    Some(("HDV/HDTV", 1280, 720, 1.0, 25.0)),
    Some(("HDV/HDTV", 1280, 720, 1.0, 29.97)),
    Some(("HDV", 1440, 1080, 4.0 / 3.0, 25.0)),
    Some(("HDV", 1440, 1080, 4.0 / 3.0, 29.97)),
    Some(("DVCPRO HD", 960, 720, 4.0 / 3.0, 23.976)),
    Some(("DVCPRO HD", 960, 720, 4.0 / 3.0, 25.0)),
    Some(("DVCPRO HD", 960, 720, 4.0 / 3.0, 29.97)),
    Some(("DVCPRO HD", 1440, 1080, 4.0 / 3.0, 25.0)),
    Some(("DVCPRO HD", 1280, 1080, 1.5, 29.97)),
    None,
    Some(("Cineon Half", 1828, 1332, 1.0, 24.0)),
    Some(("Cineon Full", 3656, 2664, 1.0, 24.0)),
    Some(("Film (2K)", 2048, 1556, 1.0, 24.0)),
    Some(("Film (4K)", 4096, 3112, 1.0, 24.0)),
];

/// The frame rate menu.
const FRAME_RATES: &[f64] = &[
    8.0, 12.0, 15.0, 23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0, 120.0,
];

/// Resolution menu: English name and pixel step (`0`: Custom).
const RESOLUTIONS: &[(&str, u32)] = &[
    ("Full", 1),
    ("Half", 2),
    ("Third", 3),
    ("Quarter", 4),
    ("Custom...", 0),
];

/// 3D renderers: English name and model token.
const RENDERERS: &[(&str, &str)] = &[
    ("Classic 3D", "classic_3d"),
    ("Advanced 3D", "advanced_3d"),
    ("Cinema 4D", "cinema_4d"),
];

/// The 3×3 anchor grid, row by row.
const ANCHORS: [ResizeAnchor; 9] = [
    ResizeAnchor::TopLeft,
    ResizeAnchor::Top,
    ResizeAnchor::TopRight,
    ResizeAnchor::Left,
    ResizeAnchor::Center,
    ResizeAnchor::Right,
    ResizeAnchor::BottomLeft,
    ResizeAnchor::Bottom,
    ResizeAnchor::BottomRight,
];

/// Maximum comp duration and start timecode magnitude (AE: 3 hours / 24 hours).
const MAX_DURATION_S: f64 = 3.0 * 3600.0;
const MAX_START_S: f64 = 24.0 * 3600.0;

/// The settings the dialog edits.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Values {
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    pub fps: f64,
    pub drop_frame: bool,
    pub resolution: [u32; 2],
    pub start: Time,
    pub duration: Time,
    pub bg: [f64; 4],
    pub shutter_angle: f64,
    pub shutter_phase: f64,
    pub samples: i64,
    pub adaptive_limit: i64,
    pub preserve_frame_rate: bool,
    pub preserve_resolution: bool,
    pub renderer: String,
}

impl Default for Values {
    /// AE's defaults for a new comp.
    fn default() -> Self {
        Values {
            width: 1920,
            height: 1080,
            pixel_aspect: 1.0,
            fps: 30.0,
            drop_frame: false,
            resolution: [1, 1],
            start: Time::ZERO,
            duration: Time::from_seconds(10.0),
            bg: [0.0, 0.0, 0.0, 1.0],
            shutter_angle: 180.0,
            shutter_phase: -90.0,
            samples: 16,
            adaptive_limit: 128,
            preserve_frame_rate: false,
            preserve_resolution: false,
            renderer: "classic_3d".into(),
        }
    }
}

impl Values {
    fn of(stage: &Stage, info: &CompInfo) -> Self {
        let view = stage.prim(&info.path);
        let get = |a: &str| view.as_ref().and_then(|v| query::static_value(v, a));
        let num = |a: &str, d: f64| get(a).and_then(|v| v.as_f64()).unwrap_or(d);
        let flag = |a: &str, d: bool| get(a).and_then(|v| v.as_bool()).unwrap_or(d);
        let resolution = match get("resolution") {
            Some(bae_model::Value::Vec2([h, v])) => [h.max(1.0) as u32, v.max(1.0) as u32],
            _ => [1, 1],
        };
        let renderer = match get("renderer") {
            Some(bae_model::Value::Enum(r)) => r,
            _ => "classic_3d".into(),
        };
        Values {
            width: info.width,
            height: info.height,
            pixel_aspect: info.pixel_aspect,
            fps: info.frame_rate.as_f64(),
            drop_frame: flag("drop_frame", false),
            resolution,
            start: info.display_start,
            duration: info.duration,
            bg: info.bg_color,
            shutter_angle: num("shutter_angle", 180.0),
            shutter_phase: num("shutter_phase", -90.0),
            samples: num("motion_blur_samples", 16.0) as i64,
            adaptive_limit: num("motion_blur_adaptive_limit", 128.0) as i64,
            preserve_frame_rate: flag("preserve_frame_rate", false),
            preserve_resolution: flag("preserve_resolution", false),
            renderer,
        }
    }

    /// The settings as `comp.create` / `comp.set_settings` parameters.
    fn params(&self) -> serde_json::Value {
        json!({
            "width": self.width,
            "height": self.height,
            "pixel_aspect": self.pixel_aspect,
            "fps": self.fps,
            "drop_frame": self.drop_frame,
            "resolution": self.resolution,
            "start_timecode": self.start,
            "duration": self.duration,
            "bg_color": self.bg,
            "shutter_angle": self.shutter_angle,
            "shutter_phase": self.shutter_phase,
            "samples_per_frame": self.samples,
            "adaptive_sample_limit": self.adaptive_limit,
            "preserve_frame_rate": self.preserve_frame_rate,
            "preserve_resolution": self.preserve_resolution,
            "renderer": self.renderer,
        })
    }

    /// Index into [`PRESETS`] of the preset these values match.
    fn preset(&self) -> Option<usize> {
        PRESETS.iter().position(|p| {
            p.is_some_and(|(_, w, h, par, fps)| {
                w == self.width
                    && h == self.height
                    && (par - self.pixel_aspect).abs() < 1e-3
                    && (fps - self.fps).abs() < 1e-3
            })
        })
    }

    fn frames(&self, t: Time) -> i64 {
        (t.as_seconds() * self.fps).round() as i64
    }

    fn time(&self, frames: i64) -> Time {
        Time::from_seconds(frames as f64 / self.fps)
    }
}

/// What New Composition starts from: the last comp created with the dialog
/// (kept across runs, like AE).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CompSettingsMemory {
    pub values: Option<Values>,
    pub lock_aspect: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    New {
        folder: Option<PrimPath>,
    },
    Edit {
        comp: PrimPath,
        initial: Box<Values>,
        name: String,
    },
}

/// The open Composition Settings dialog.
#[derive(Clone, Debug)]
pub struct CompSettings {
    target: Target,
    pub name: String,
    pub values: Values,
    pub tab: usize,
    pub lock_aspect: bool,
    /// Width / height when the lock was turned on.
    aspect: f64,
    pub anchor: ResizeAnchor,
    fps_text: Option<String>,
    preview: bool,
    /// The name field got its initial focus (AE opens with the name selected).
    name_focused: bool,
}

impl CompSettings {
    /// New Composition: the next "Comp N" with the remembered settings.
    pub(crate) fn new_comp(
        stage: &Stage,
        session: &SharedSession,
        memory: &CompSettingsMemory,
        folder: Option<PrimPath>,
    ) -> Self {
        let values = memory.values.clone().unwrap_or_default();
        let aspect = f64::from(values.width) / f64::from(values.height);
        CompSettings {
            name: next_comp_name(stage, session, folder.as_ref()),
            target: Target::New { folder },
            values,
            tab: 0,
            lock_aspect: memory.lock_aspect,
            aspect,
            anchor: ResizeAnchor::Center,
            fps_text: None,
            preview: false,
            name_focused: false,
        }
    }

    /// Composition Settings of an existing comp.
    pub(crate) fn edit(stage: &Stage, comp: &PrimPath) -> Option<Self> {
        let info = query::comp_info(stage, comp)?;
        let values = Values::of(stage, &info);
        let aspect = f64::from(values.width) / f64::from(values.height);
        Some(CompSettings {
            name: info.name.clone(),
            target: Target::Edit {
                comp: comp.clone(),
                initial: Box::new(values.clone()),
                name: info.name,
            },
            values,
            tab: 0,
            lock_aspect: false,
            aspect,
            anchor: ResizeAnchor::Center,
            fps_text: None,
            preview: true,
            name_focused: false,
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
        let id = Id::new("bae-ui-motion.comp-settings");
        let shown = dialog::window(ctx, id, tr("Composition Settings"), |ui| self.body(ui, id));
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
        !self.name.trim().is_empty()
    }

    fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession, dialogs: &SharedDialogs) {
        let name = self.name.trim().to_owned();
        match &self.target {
            Target::New { folder } => {
                let mut params = self.values.params();
                params["name"] = json!(name);
                let prefs = crate::prefs::current(cx.resources);
                params["label"] = json!(crate::prefs::comp_label(&prefs));
                let parent = folder
                    .clone()
                    .unwrap_or_else(|| PrimPath::new(query::COMPS_FOLDER).expect("valid"));
                if let Some(f) = folder {
                    params["parent"] = json!(f);
                }
                let path = parent
                    .child(&query::unique_child_name(cx.stage, &parent, &name))
                    .ok();
                cx.invoke_with("comp.create", params);
                if let Some(path) = path {
                    let mut s = session.lock();
                    s.expect(Pending::Comp(path.clone()));
                    s.reserve_name(path, name);
                }
                let mut d = dialogs.lock();
                d.comp.values = Some(self.values.clone());
                d.comp.lock_aspect = self.lock_aspect;
                super::memory::save(&d);
            }
            Target::Edit {
                comp,
                initial,
                name: old_name,
            } => {
                let mut params = json!({ "comp": comp });
                if name != *old_name {
                    params["name"] = json!(name);
                }
                let new = self.values.params();
                let old = initial.params();
                if let (Some(n), Some(o)) = (new.as_object(), old.as_object()) {
                    for (k, v) in n {
                        if o.get(k) != Some(v) {
                            params[k] = v.clone();
                        }
                    }
                }
                if params.get("width").is_some() || params.get("height").is_some() {
                    params["width"] = json!(self.values.width);
                    params["height"] = json!(self.values.height);
                    params["anchor"] = json!(self.anchor);
                }
                if params.as_object().is_some_and(|m| m.len() > 1) {
                    cx.invoke_with("comp.set_settings", params);
                }
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_width(520.0);
        ui.horizontal(|ui| {
            ui.label(tr("Composition Name:"));
            fields::name_field(
                ui,
                id.with("name"),
                &mut self.name,
                300.0,
                tr("Composition Name"),
                &mut self.name_focused,
            );
        });
        ui.add_space(4.0);
        tabs(ui, &mut self.tab, &["Basic", "Advanced", "3D Renderer"]);
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, color::DIVIDER))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_min_height(300.0);
                ui.set_width(ui.available_width());
                match self.tab {
                    0 => self.basic(ui, id),
                    1 => self.advanced(ui),
                    _ => self.renderer(ui),
                }
            });
        let mut out = Outcome::None;
        ui.horizontal(|ui| {
            // Live preview of edits is not implemented yet: shown like AE, off.
            ui.add_enabled_ui(false, |ui| {
                bae_ui::widgets::checkbox(ui, &mut self.preview, tr("Preview"));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                out = dialog::buttons(ui, self.valid());
            });
        });
        out
    }

    fn basic(&mut self, ui: &mut Ui, id: Id) {
        const LW: f32 = 128.0;
        let v = &mut self.values;
        // Preset
        dialog::row(ui, LW, tr("Preset:"), |ui| {
            let current = v.preset();
            let label = current.map_or_else(|| tr("Custom").to_owned(), preset_label);
            let mut items: Vec<Option<String>> = vec![Some(tr("Custom").to_owned()), None];
            items.extend(
                PRESETS
                    .iter()
                    .enumerate()
                    .map(|(i, p)| p.map(|_| preset_label(i))),
            );
            if let Some(picked) =
                menu_field(ui, id.with("preset"), &label, &items, 300.0, tr("Preset"))
                && picked >= 2
                && let Some(Some((_, w, h, par, fps))) = PRESETS.get(picked - 2)
            {
                v.width = *w;
                v.height = *h;
                v.pixel_aspect = *par;
                v.fps = *fps;
                self.fps_text = None;
            }
            // Saving and deleting presets are not implemented yet: greyed.
            preset_buttons(ui);
        });
        ui.add_space(4.0);
        // Width / height with the aspect lock.
        let (mut w, mut h) = (f64::from(v.width), f64::from(v.height));
        let ratio = frame_aspect_text(v.width, v.height, 1.0);
        dialog::row(ui, LW, tr("Width:"), |ui| {
            if fields::number(ui, &mut w, 1.0..=30000.0, 0, tr("px"), tr("Width")).changed() {
                v.width = w as u32;
                if self.lock_aspect {
                    v.height = ((w / self.aspect).round() as u32).clamp(1, 30000);
                }
            }
            ui.add_space(24.0);
            let mut lock = self.lock_aspect;
            let label = tr("Lock Aspect Ratio to {ratio}").replace("{ratio}", &ratio);
            if bae_ui::widgets::checkbox(ui, &mut lock, &label).changed() {
                self.lock_aspect = lock;
                self.aspect = f64::from(v.width) / f64::from(v.height);
            }
        });
        dialog::row(ui, LW, tr("Height:"), |ui| {
            if fields::number(ui, &mut h, 1.0..=30000.0, 0, tr("px"), tr("Height")).changed() {
                v.height = h as u32;
                if self.lock_aspect {
                    v.width = ((h * self.aspect).round() as u32).clamp(1, 30000);
                }
            }
        });
        ui.add_space(4.0);
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
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(tr("Frame Aspect Ratio:")).color(color::TEXT_DIM));
                ui.label(
                    egui::RichText::new(frame_aspect_text(v.width, v.height, v.pixel_aspect))
                        .color(color::TEXT),
                );
            });
        });
        dialog::row(ui, LW, tr("Frame Rate:"), |ui| {
            let mut text = self.fps_text.clone().unwrap_or_else(|| fps_text(v.fps));
            let r = ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(52.0)
                        .text_color(color::TEXT_STRONG),
                )
                .labeled(egui::WidgetType::TextEdit, tr("Frame Rate"));
            if r.changed() {
                self.fps_text = Some(text.clone());
            }
            if r.lost_focus() {
                if let Ok(f) = text.trim().parse::<f64>()
                    && (1.0..=999.0).contains(&f)
                {
                    let (start, dur) = (v.start, v.duration);
                    v.fps = f;
                    v.start = start;
                    v.duration = dur;
                }
                self.fps_text = None;
            }
            let items: Vec<Option<String>> =
                FRAME_RATES.iter().map(|f| Some(fps_text(*f))).collect();
            if let Some(i) = menu_button(ui, id.with("fps-menu"), &items, tr("Frame Rate Presets"))
            {
                v.fps = FRAME_RATES[i];
                self.fps_text = None;
            }
            ui.label(tr("frames per second"));
            let df = fields::supports_drop_frame(v.fps);
            ui.add_enabled_ui(df, |ui| {
                let names = [tr("Drop Frame").to_owned(), tr("Non-Drop Frame").to_owned()];
                let label = if v.drop_frame && df {
                    &names[0]
                } else {
                    &names[1]
                };
                let items: Vec<Option<String>> = names.iter().cloned().map(Some).collect();
                if let Some(i) = menu_field(ui, id.with("df"), label, &items, 120.0, tr("Timecode"))
                {
                    v.drop_frame = i == 0;
                }
            });
        });
        dialog::row(ui, LW, tr("Resolution:"), |ui| {
            let current = RESOLUTIONS
                .iter()
                .position(|(_, s)| *s != 0 && v.resolution == [*s, *s])
                .unwrap_or(RESOLUTIONS.len() - 1);
            let items: Vec<Option<String>> = RESOLUTIONS
                .iter()
                .map(|(n, _)| Some(tr(n).to_owned()))
                .collect();
            let label = tr(RESOLUTIONS[current].0).to_owned();
            if let Some(i) = menu_field(ui, id.with("res"), &label, &items, 110.0, tr("Resolution"))
            {
                let s = RESOLUTIONS[i].1;
                if s != 0 {
                    v.resolution = [s, s];
                }
            }
            if current == RESOLUTIONS.len() - 1 {
                let mut hstep = f64::from(v.resolution[0]);
                let mut vstep = f64::from(v.resolution[1]);
                if fields::number(ui, &mut hstep, 1.0..=99.0, 0, "", tr("Horizontal")).changed() {
                    v.resolution[0] = hstep as u32;
                }
                if fields::number(ui, &mut vstep, 1.0..=99.0, 0, "", tr("Vertical")).changed() {
                    v.resolution[1] = vstep as u32;
                }
            }
            let (rw, rh) = (
                v.width.div_ceil(v.resolution[0].max(1)),
                v.height.div_ceil(v.resolution[1].max(1)),
            );
            let mb = f64::from(rw) * f64::from(rh) * 4.0 / (1024.0 * 1024.0);
            ui.label(
                egui::RichText::new(
                    tr("{w} x {h}, {mb} MB per 8bpc frame")
                        .replace("{w}", &rw.to_string())
                        .replace("{h}", &rh.to_string())
                        .replace("{mb}", &format!("{mb:.1}")),
                )
                .color(color::TEXT_DIM),
            );
        });
        ui.add_space(4.0);
        let df = v.drop_frame && fields::supports_drop_frame(v.fps);
        let base = fields::timecode_base(v.fps);
        let fps = v.fps;
        let is_text = move |frames: i64| {
            tr("is {tc} Base {n}")
                .replace("{tc}", &fields::format_timecode(frames, fps, df))
                .replace("{n}", &base.to_string())
        };
        dialog::row(ui, LW, tr("Start Timecode:"), |ui| {
            let mut f = v.frames(v.start);
            let max = (MAX_START_S * v.fps) as i64;
            if fields::timecode_field(
                ui,
                Id::new("cs.start"),
                &mut f,
                v.fps,
                df,
                (-max, max),
                tr("Start Timecode"),
            )
            .changed()
            {
                v.start = v.time(f);
            }
            ui.label(egui::RichText::new(is_text(f)).color(color::TEXT_DIM));
        });
        dialog::row(ui, LW, tr("Duration:"), |ui| {
            let mut f = v.frames(v.duration).max(1);
            let max = (MAX_DURATION_S * v.fps) as i64;
            if fields::timecode_field(
                ui,
                Id::new("cs.duration"),
                &mut f,
                v.fps,
                df,
                (1, max),
                tr("Duration"),
            )
            .changed()
            {
                v.duration = v.time(f);
            }
            ui.label(egui::RichText::new(is_text(f)).color(color::TEXT_DIM));
        });
        ui.add_space(4.0);
        dialog::row(ui, LW, tr("Background Color:"), |ui| {
            let title = tr("Background Color");
            if let ColorEdit::Commit(c) =
                color_picker::color_button(ui, "comp-bg", &mut v.bg, vec2(36.0, 16.0), title, title)
            {
                v.bg = c;
            }
            if let Some(rgb) = color_picker::eyedropper_button(ui, id.with("bg-dropper")) {
                v.bg = [rgb[0], rgb[1], rgb[2], 1.0];
            }
            if v.bg[..3].iter().all(|c| *c == 0.0) {
                ui.label(egui::RichText::new(tr("Black")).color(color::TEXT));
            }
        });
    }

    fn advanced(&mut self, ui: &mut Ui) {
        const LW: f32 = 128.0;
        let editing = !self.is_new();
        dialog::row(ui, LW, tr("Anchor:"), |ui| {
            // AE: only when editing, and only once the size changed.
            let resized = match &self.target {
                Target::Edit { initial, .. } => {
                    (initial.width, initial.height) != (self.values.width, self.values.height)
                }
                Target::New { .. } => false,
            };
            ui.add_enabled_ui(editing && resized, |ui| anchor_grid(ui, &mut self.anchor));
        });
        ui.add_space(6.0);
        let v = &mut self.values;
        bae_ui::widgets::checkbox(
            ui,
            &mut v.preserve_frame_rate,
            tr("Preserve frame rate when nested or in render queue"),
        );
        bae_ui::widgets::checkbox(
            ui,
            &mut v.preserve_resolution,
            tr("Preserve resolution when nested"),
        );
        dialog::group(ui, tr("Motion Blur"), |ui| {
            dialog::row(ui, LW - 20.0, tr("Shutter Angle:"), |ui| {
                ui.add(
                    egui::Slider::new(&mut v.shutter_angle, 0.0..=720.0)
                        .show_value(false)
                        .integer(),
                )
                .labeled(egui::WidgetType::Slider, tr("Shutter Angle"));
                fields::number(
                    ui,
                    &mut v.shutter_angle,
                    0.0..=720.0,
                    0,
                    "°",
                    tr("Shutter Angle Value"),
                );
            });
            dialog::row(ui, LW - 20.0, tr("Shutter Phase:"), |ui| {
                ui.add(
                    egui::Slider::new(&mut v.shutter_phase, -360.0..=360.0)
                        .show_value(false)
                        .integer(),
                )
                .labeled(egui::WidgetType::Slider, tr("Shutter Phase"));
                fields::number(
                    ui,
                    &mut v.shutter_phase,
                    -360.0..=360.0,
                    0,
                    "°",
                    tr("Shutter Phase Value"),
                );
            });
            dialog::row(ui, LW - 20.0, tr("Samples Per Frame:"), |ui| {
                let mut s = v.samples as f64;
                if fields::number(ui, &mut s, 2.0..=64.0, 0, "", tr("Samples Per Frame")).changed()
                {
                    v.samples = s as i64;
                }
                ui.add_space(16.0);
                ui.label(tr("Adaptive Sample Limit:"));
                let mut a = v.adaptive_limit as f64;
                if fields::number(ui, &mut a, 64.0..=256.0, 0, "", tr("Adaptive Sample Limit"))
                    .changed()
                {
                    v.adaptive_limit = a as i64;
                }
            });
        });
    }

    fn renderer(&mut self, ui: &mut Ui) {
        let v = &mut self.values;
        dialog::row(ui, 80.0, tr("Renderer:"), |ui| {
            let current = RENDERERS
                .iter()
                .position(|(_, t)| *t == v.renderer)
                .unwrap_or(0);
            let items: Vec<Option<String>> = RENDERERS
                .iter()
                .map(|(n, _)| Some(tr(n).to_owned()))
                .collect();
            let label = tr(RENDERERS[current].0).to_owned();
            if let Some(i) = menu_field(
                ui,
                Id::new("cs.renderer"),
                &label,
                &items,
                160.0,
                tr("Renderer"),
            ) {
                v.renderer = RENDERERS[i].1.to_owned();
            }
            // The renderer options and About windows are not implemented yet.
            ui.add_enabled_ui(false, |ui| {
                let _ = ui.button(tr("Options..."));
                let _ = ui.button(tr("About..."));
            });
        });
        ui.add_space(8.0);
        let about = match v.renderer.as_str() {
            "cinema_4d" => "Extruded text and shapes, reflections; slower than Classic 3D.",
            "advanced_3d" => "GPU-accelerated 3D with lights, shadows and 3D models.",
            _ => "Layers as planes in 3D space; supports all 2D features.",
        };
        ui.label(egui::RichText::new(tr(about)).color(color::TEXT_DIM));
    }
}

/// Localized label of `PRESETS[i]`: "HD  •  1920x1080 • 29.97 fps".
fn preset_label(i: usize) -> String {
    let Some(Some((name, w, h, par, fps))) = PRESETS.get(i) else {
        return String::new();
    };
    let size = if (par - 1.0).abs() < 1e-6 {
        format!("{w}x{h}")
    } else {
        format!("{w}x{h} ({par:.2})")
    };
    format!("{}  •  {size} • {} fps", tr(name), fps_text(*fps))
}

/// "29.97", "30", "23.976", "12.5".
pub(crate) fn fps_text(fps: f64) -> String {
    let s = format!("{fps:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// "16:9 (1.78)": the frame aspect ratio of a size with a pixel aspect ratio.
pub(crate) fn frame_aspect_text(width: u32, height: u32, pixel_aspect: f64) -> String {
    let w = (f64::from(width) * pixel_aspect).round() as u64;
    let h = u64::from(height.max(1));
    let g = gcd(w.max(1), h);
    let (a, b) = (w / g, h / g);
    let ratio = w as f64 / h as f64;
    if a <= 99 && b <= 99 {
        format!("{a}:{b} ({ratio:.2})")
    } else {
        format!("{ratio:.2}")
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The next "Comp N" (localized), counting comps commands are about to create.
pub(crate) fn next_comp_name(
    stage: &Stage,
    session: &SharedSession,
    folder: Option<&PrimPath>,
) -> String {
    let base = tr("Comp");
    let number = |n: &str| -> Option<u32> { n.strip_prefix(base)?.trim().parse().ok() };
    let max = query::comps(stage)
        .iter()
        .filter_map(|c| number(&query::display_name_of(stage, c)))
        .max()
        .unwrap_or(0);
    let parent = folder
        .cloned()
        .unwrap_or_else(|| PrimPath::new(query::COMPS_FOLDER).expect("valid"));
    let reserved = session
        .lock()
        .reserved_names(&parent)
        .filter_map(|n| number(&n))
        .max()
        .unwrap_or(0);
    format!("{base} {}", max.max(reserved) + 1)
}

/// The greyed Save / Delete preset buttons (floppy disk, trash can).
pub(crate) fn preset_buttons(ui: &mut Ui) {
    for (icon, label) in [
        (bae_ui::Icon::Save, "Save Preset"),
        (bae_ui::Icon::Trash, "Delete Preset"),
    ] {
        let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(18.0), Sense::hover());
        let _ = resp.labeled(egui::WidgetType::Button, tr(label));
        bae_ui::icons::paint(ui.painter(), rect.shrink(2.0), icon, color::TEXT_DISABLED);
    }
}

/// AE dialog tabs: a row of names, the selected one underlined.
fn tabs(ui: &mut Ui, selected: &mut usize, names: &[&str]) {
    ui.horizontal(|ui| {
        for (i, name) in names.iter().enumerate() {
            let text = tr(name);
            let on = *selected == i;
            let galley = ui.painter().layout_no_wrap(
                text.to_owned(),
                bae_ui::theme::body(),
                if on { color::TEXT_STRONG } else { color::TEXT },
            );
            let (rect, resp) =
                ui.allocate_exact_size(galley.size() + vec2(16.0, 10.0), Sense::click());
            let resp = resp.labeled_toggle(egui::WidgetType::RadioButton, text, on);
            if resp.clicked() {
                *selected = i;
            }
            let p = ui.painter();
            p.galley(rect.center() - galley.size() / 2.0, galley, color::TEXT);
            if on {
                p.hline(
                    rect.x_range(),
                    rect.bottom() - 1.0,
                    egui::Stroke::new(2.0, color::ACCENT),
                );
            }
        }
    });
}

/// A dropdown field (dark, chevron) whose menu may have separators (`None`).
/// Returns the picked index.
pub(crate) fn menu_field(
    ui: &mut Ui,
    id: Id,
    current: &str,
    items: &[Option<String>],
    width: f32,
    label: &str,
) -> Option<usize> {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click());
    let resp = resp.labeled_value(egui::WidgetType::ComboBox, label, current);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let enabled = ui.is_enabled();
        let bg = if resp.hovered() {
            color::FIELD_HOVER
        } else {
            color::DROPDOWN
        };
        p.rect_filled(rect, 3.0, bg);
        let fg = if enabled {
            color::TEXT_STRONG
        } else {
            color::TEXT_DISABLED
        };
        p.with_clip_rect(rect.shrink2(vec2(4.0, 0.0)).with_max_x(rect.right() - 18.0))
            .text(
                egui::pos2(rect.left() + 7.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                current,
                bae_ui::theme::body(),
                fg,
            );
        let chev = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Vec2::splat(12.0),
        );
        bae_ui::icons::paint(p, chev, bae_ui::Icon::ChevronDown, fg);
    }
    popup_items(&resp, id, items, width.max(160.0))
}

/// A small chevron button opening a menu (the frame rate presets).
fn menu_button(ui: &mut Ui, id: Id, items: &[Option<String>], label: &str) -> Option<usize> {
    let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::click());
    let resp = resp.labeled(egui::WidgetType::Button, label);
    let chev = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(12.0));
    bae_ui::icons::paint(
        ui.painter(),
        chev,
        bae_ui::Icon::ChevronDown,
        color::TEXT_STRONG,
    );
    popup_items(&resp, id, items, 90.0)
}

fn popup_items(
    resp: &egui::Response,
    id: Id,
    items: &[Option<String>],
    width: f32,
) -> Option<usize> {
    let mut picked = None;
    egui::Popup::menu(resp).id(id).width(width).show(|ui| {
        for (i, item) in items.iter().enumerate() {
            match item {
                Some(text) => {
                    if ui.button(text).clicked() {
                        picked = Some(i);
                    }
                }
                None => {
                    ui.separator();
                }
            }
        }
    });
    picked
}

/// The Advanced tab's 3×3 anchor grid of arrow buttons.
fn anchor_grid(ui: &mut Ui, anchor: &mut ResizeAnchor) {
    const NAMES: [&str; 9] = [
        "Top Left",
        "Top",
        "Top Right",
        "Left",
        "Center",
        "Right",
        "Bottom Left",
        "Bottom",
        "Bottom Right",
    ];
    let cell = 18.0;
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(cell * 3.0 + 4.0), Sense::hover());
    let enabled = ui.is_enabled();
    for (i, a) in ANCHORS.iter().enumerate() {
        let (r, c) = ((i / 3) as f32, (i % 3) as f32);
        let cr = egui::Rect::from_min_size(
            rect.min + vec2(c * (cell + 2.0), r * (cell + 2.0)),
            egui::Vec2::splat(cell),
        );
        let resp = ui
            .interact(cr, ui.id().with(("anchor", i)), Sense::click())
            .labeled_toggle(
                egui::WidgetType::RadioButton,
                &bae_ui::a11y::context(&[tr("Anchor"), tr(NAMES[i])]),
                anchor == a,
            );
        if resp.clicked() && enabled {
            *anchor = *a;
        }
        let p = ui.painter();
        let on = anchor == a;
        p.rect_filled(cr, 2.0, if on { color::ACCENT_DIM } else { color::FIELD });
        p.rect_stroke(
            cr,
            2.0,
            egui::Stroke::new(1.0, color::DIVIDER),
            egui::StrokeKind::Inside,
        );
        // An arrow pointing away from the selected cell (where the frame grows).
        let sel = ANCHORS.iter().position(|x| x == anchor).unwrap_or(4);
        let (dr, dc) = (r - (sel / 3) as f32, c - (sel % 3) as f32);
        let fg = if enabled {
            color::TEXT_STRONG
        } else {
            color::TEXT_DISABLED
        };
        if on {
            p.circle_filled(cr.center(), 3.0, fg);
        } else if dr.abs() <= 1.0 && dc.abs() <= 1.0 {
            let dir = vec2(dc, dr).normalized() * 5.0;
            p.arrow(cr.center() - dir, dir * 2.0, egui::Stroke::new(1.2, fg));
        }
    }
}

/// English strings this dialog shows (for the i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Composition Settings",
    "Composition Name:",
    "Composition Name",
    "Basic",
    "Advanced",
    "3D Renderer",
    "Preview",
    "Preset:",
    "Preset",
    "Custom",
    "Save Preset",
    "Delete Preset",
    "Width:",
    "Width",
    "Height:",
    "Height",
    "px",
    "Lock Aspect Ratio to {ratio}",
    "Pixel Aspect Ratio:",
    "Pixel Aspect Ratio",
    "Frame Aspect Ratio:",
    "Frame Rate:",
    "Frame Rate",
    "Frame Rate Presets",
    "frames per second",
    "Drop Frame",
    "Non-Drop Frame",
    "Timecode",
    "Resolution:",
    "Resolution",
    "Full",
    "Half",
    "Third",
    "Quarter",
    "Custom...",
    "Horizontal",
    "Vertical",
    "{w} x {h}, {mb} MB per 8bpc frame",
    "is {tc} Base {n}",
    "Start Timecode:",
    "Start Timecode",
    "Duration:",
    "Duration",
    "Background Color:",
    "Background Color",
    "Black",
    "Anchor:",
    "Anchor",
    "Top Left",
    "Top",
    "Top Right",
    "Left",
    "Center",
    "Right",
    "Bottom Left",
    "Bottom",
    "Bottom Right",
    "Preserve frame rate when nested or in render queue",
    "Preserve resolution when nested",
    "Motion Blur",
    "Shutter Angle:",
    "Shutter Angle",
    "Shutter Angle Value",
    "Shutter Phase:",
    "Shutter Phase",
    "Shutter Phase Value",
    "Samples Per Frame:",
    "Samples Per Frame",
    "Adaptive Sample Limit:",
    "Adaptive Sample Limit",
    "Renderer:",
    "Renderer",
    "Classic 3D",
    "Cinema 4D",
    "Advanced 3D",
    "Options...",
    "About...",
    "Extruded text and shapes, reflections; slower than Classic 3D.",
    "GPU-accelerated 3D with lights, shadows and 3D models.",
    "Layers as planes in 3D space; supports all 2D features.",
    "Square Pixels",
    "D1/DV NTSC (0.91)",
    "D1/DV NTSC Widescreen (1.21)",
    "D1/DV PAL (1.09)",
    "D1/DV PAL Widescreen (1.46)",
    "Anamorphic 2:1 (2)",
    "HDV 1080/DVCPRO HD 720 (1.33)",
    "DVCPRO HD 1080 (1.5)",
    "Social Media Portrait HD",
    "Social Media Landscape HD",
    "Social Media Portrait",
    "Social Media Landscape",
    "Social Media Square",
    "Cineon Half",
    "Cineon Full",
    "Film (2K)",
    "Film (4K)",
    "Comp",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_labels() {
        assert_eq!(fps_text(29.97), "29.97");
        assert_eq!(fps_text(30.0), "30");
        assert_eq!(fps_text(23.976), "23.976");
        assert_eq!(fps_text(12.5), "12.5");
        assert_eq!(frame_aspect_text(1920, 1080, 1.0), "16:9 (1.78)");
        assert_eq!(frame_aspect_text(1440, 1080, 4.0 / 3.0), "16:9 (1.78)");
        assert_eq!(frame_aspect_text(1080, 1080, 1.0), "1:1 (1.00)");
        assert_eq!(frame_aspect_text(1828, 1332, 1.0), "1.37");
        let v = Values {
            fps: 29.97,
            ..Values::default()
        };
        assert_eq!(v.preset(), Some(2));
        assert_eq!(preset_label(2), "HD  •  1920x1080 • 29.97 fps");
        assert_eq!(preset_label(17), "HDV  •  1440x1080 (1.33) • 25 fps");
        // The default (1920x1080, 30 fps) is Social Media Landscape HD.
        assert_eq!(Values::default().preset(), Some(10));
    }
}
