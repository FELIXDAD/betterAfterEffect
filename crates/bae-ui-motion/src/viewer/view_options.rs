//! What the Composition panel shows around the comp: View ▸ View Options
//! (Cmd/Ctrl+Alt+U, which layer controls are drawn), View ▸ Panel Background
//! Color (the pasteboard) and View ▸ Resolution ▸ Custom… (AE's Custom
//! Resolution dialog).
//!
//! The choices live in the session, so every viewer follows them, like AE's
//! menu applies them to the Composition panel.

use bae_app::Resources;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{Invocation, PanelContext, tr};
use egui::{Color32, Id, Ui};

use crate::common;
use crate::dialogs::{self, Dialog};

/// Catalog action ids.
pub(crate) mod ids {
    pub const VIEW_OPTIONS: &str = "motion.view.view_options";
    /// `{"color": "black" | "dark_gray" | "medium_gray" | "light_gray" | "white" | "custom"}`.
    pub const PANEL_BACKGROUND: &str = "motion.view.panel_background_color";
    pub const CUSTOM_BACKGROUND: &str = "motion.view.select_custom_background_color";
    pub const CUSTOM_RESOLUTION: &str = "motion.view.resolution_custom";
}

/// `(action id, English title)` for the action registry (AE's names).
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    (ids::VIEW_OPTIONS, "View Options"),
    (ids::PANEL_BACKGROUND, "Panel Background Color"),
    (ids::CUSTOM_BACKGROUND, "Select Custom Background Color..."),
    (ids::CUSTOM_RESOLUTION, "Custom Resolution"),
];

/// The Set Composition Resolution to Custom shortcuts: `(params, English
/// title)` (AE's names; one row each in the shortcut editor).
pub(crate) const VARIANTS: &[(&str, &str)] = &[
    ("null", "Set Composition Resolution to Custom"),
    (
        r#"{"nested":true}"#,
        "Set Composition Resolution to Custom for the Focused and Nested Comps",
    ),
];

/// When a camera or light wireframe is drawn in the 3D views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wireframes {
    On,
    Off,
    #[default]
    WhenSelected,
}

impl Wireframes {
    const ALL: [Wireframes; 3] = [Wireframes::On, Wireframes::Off, Wireframes::WhenSelected];

    fn title(self) -> &'static str {
        match self {
            Wireframes::On => "On",
            Wireframes::Off => "Off",
            Wireframes::WhenSelected => "When Selected",
        }
    }

    /// Whether a camera or light drawn `selected` shows its wireframe.
    pub fn shows(self, selected: bool) -> bool {
        match self {
            Wireframes::On => true,
            Wireframes::Off => false,
            Wireframes::WhenSelected => selected,
        }
    }
}

/// View Options: the layer controls the Composition panel draws (AE's
/// defaults: all on, wireframes when selected).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerControls {
    pub handles: bool,
    pub masks: bool,
    pub mask_feather: bool,
    pub effect_controls: bool,
    pub keyframes: bool,
    pub motion_paths: bool,
    pub tangents: bool,
    pub gizmo_3d: bool,
    pub cameras: Wireframes,
    pub lights: Wireframes,
    pub pixel_aspect: bool,
}

impl Default for LayerControls {
    fn default() -> Self {
        LayerControls {
            handles: true,
            masks: true,
            mask_feather: true,
            effect_controls: true,
            keyframes: true,
            motion_paths: true,
            tangents: true,
            gizmo_3d: true,
            cameras: Wireframes::WhenSelected,
            lights: Wireframes::WhenSelected,
            pixel_aspect: false,
        }
    }
}

/// View ▸ Panel Background Color: the color around the comp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Background {
    Black,
    DarkGray,
    #[default]
    MediumGray,
    LightGray,
    White,
    Custom,
}

impl Background {
    const ALL: [(Background, &'static str); 6] = [
        (Background::Black, "black"),
        (Background::DarkGray, "dark_gray"),
        (Background::MediumGray, "medium_gray"),
        (Background::LightGray, "light_gray"),
        (Background::White, "white"),
        (Background::Custom, "custom"),
    ];

    fn parse(key: &str) -> Option<Background> {
        Self::ALL.iter().find(|(_, k)| *k == key).map(|(b, _)| *b)
    }
}

/// The pasteboard choice and the custom color (8-bit RGB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelBackground {
    pub choice: Background,
    pub custom: [u8; 3],
}

impl Default for PanelBackground {
    fn default() -> Self {
        PanelBackground {
            choice: Background::MediumGray,
            custom: MEDIUM_GRAY,
        }
    }
}

/// The default pasteboard (AE's Medium Gray).
const MEDIUM_GRAY: [u8; 3] = [35, 35, 35];

impl PanelBackground {
    pub fn color(self) -> Color32 {
        let [r, g, b] = match self.choice {
            Background::Black => [0, 0, 0],
            Background::DarkGray => [20, 20, 20],
            Background::MediumGray => MEDIUM_GRAY,
            Background::LightGray => [80, 80, 80],
            Background::White => [255, 255, 255],
            Background::Custom => self.custom,
        };
        Color32::from_rgb(r, g, b)
    }
}

/// The session's view options.
pub(crate) fn layer_controls(resources: &Resources) -> LayerControls {
    common::session(resources).lock().layer_controls
}

pub(crate) fn panel_background(resources: &Resources) -> PanelBackground {
    common::session(resources).lock().panel_background
}

/// Whether the Panel Background Color item for `key` is the current one.
pub(crate) fn background_checked(resources: &Resources, key: &str) -> bool {
    Background::parse(key) == Some(panel_background(resources).choice)
}

/// Runs the view option commands; `false` for other actions.
pub(crate) fn handle(inv: &Invocation, cx: &PanelContext<'_>) -> bool {
    match inv.id.as_str() {
        ids::VIEW_OPTIONS => {
            let current = layer_controls(cx.resources);
            dialogs::open(cx.resources, Dialog::ViewOptions(Box::new(current)));
        }
        ids::PANEL_BACKGROUND => {
            let key = inv.params.get("color").and_then(|v| v.as_str());
            if let Some(b) = key.and_then(Background::parse) {
                common::session(cx.resources).lock().panel_background.choice = b;
            }
        }
        ids::CUSTOM_BACKGROUND => {
            let custom = panel_background(cx.resources).custom;
            dialogs::open(cx.resources, Dialog::BackgroundColor(custom));
        }
        ids::CUSTOM_RESOLUTION => {
            let session = common::session(cx.resources);
            let Some(comp) = session.lock().active_comp.clone() else {
                return true;
            };
            let steps = super::comp_steps(cx.stage, &comp);
            let nested = inv.params.get("nested").and_then(|v| v.as_bool()) == Some(true);
            dialogs::open(cx.resources, Dialog::CustomResolution { steps, nested });
        }
        _ => return false,
    }
    true
}

/// Draw View Options; returns whether it stays open.
pub(crate) fn view_options_ui(
    ctx: &egui::Context,
    cx: &PanelContext<'_>,
    v: &mut LayerControls,
) -> bool {
    let id = Id::new("bae-ui-motion.view-options");
    let shown = dialog::window(ctx, id, tr("View Options"), |ui| {
        view_options_body(ui, id, v)
    });
    match outcome(ctx, id, shown) {
        Outcome::None => true,
        Outcome::Cancel => false,
        Outcome::Ok => {
            common::session(cx.resources).lock().layer_controls = *v;
            false
        }
    }
}

fn outcome(ctx: &egui::Context, id: Id, shown: Option<Outcome>) -> Outcome {
    match shown {
        Some(o) => o,
        None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
        None => Outcome::None,
    }
}

fn view_options_body(ui: &mut Ui, id: Id, v: &mut LayerControls) -> Outcome {
    ui.set_width(300.0);
    let check = |ui: &mut Ui, on: &mut bool, label: &str| {
        bae_ui::widgets::checkbox(ui, on, tr(label));
    };
    dialog::group(ui, tr("Layer Controls"), |ui| {
        check(ui, &mut v.handles, "Handles");
        check(ui, &mut v.masks, "Masks");
        ui.indent(id.with("feather"), |ui| {
            ui.add_enabled_ui(v.masks, |ui| {
                check(ui, &mut v.mask_feather, "Mask Feather Boundaries");
            });
        });
        check(ui, &mut v.effect_controls, "Effect Controls");
        check(ui, &mut v.keyframes, "Keyframes");
        check(ui, &mut v.motion_paths, "Motion Paths");
        check(ui, &mut v.tangents, "Motion Path Tangents");
        check(ui, &mut v.gizmo_3d, "3D Transform Gizmo");
        ui.add_space(4.0);
        for (label, w, key) in [
            ("Camera Wireframes -", &mut v.cameras, "cameras"),
            ("Spot Light Wireframes -", &mut v.lights, "lights"),
        ] {
            ui.horizontal(|ui| {
                ui.label(tr(label));
                let titles: Vec<&str> = Wireframes::ALL.iter().map(|w| tr(w.title())).collect();
                let mut i = Wireframes::ALL.iter().position(|x| x == w).unwrap_or(2);
                if bae_ui::widgets::dropdown(ui, id.with(key), &mut i, &titles, 110.0).changed() {
                    *w = Wireframes::ALL[i];
                }
            });
        }
    });
    dialog::group(ui, tr("View"), |ui| {
        check(ui, &mut v.pixel_aspect, "Pixel Aspect Ratio Correction");
    });
    ui.add_space(4.0);
    dialog::buttons(ui, true)
}

/// Select Custom Background Color…: AE's color picker; OK makes Custom the
/// panel background.
pub(crate) fn background_color_ui(
    ctx: &egui::Context,
    cx: &PanelContext<'_>,
    custom: [u8; 3],
) -> bool {
    let c = custom.map(|x| f64::from(x) / 255.0);
    let id = Id::new("bae-ui-motion.panel-background");
    let title = tr("Select Custom Panel Background Color");
    match bae_ui::color_picker::picker(ctx, id, title, [c[0], c[1], c[2], 1.0]) {
        bae_ui::color_picker::ColorEdit::Commit(rgba) => {
            let rgb = bae_ui::color_picker::to_8bit([rgba[0], rgba[1], rgba[2]]);
            let session = common::session(cx.resources);
            let mut s = session.lock();
            s.panel_background.custom = rgb;
            s.panel_background.choice = Background::Custom;
            false
        }
        bae_ui::color_picker::ColorEdit::Cancel => false,
        _ => true,
    }
}

/// Custom Resolution: render every n-th pixel horizontally and vertically.
pub(crate) fn custom_resolution_ui(
    ctx: &egui::Context,
    cx: &mut PanelContext<'_>,
    steps: &mut [u32; 2],
    nested: bool,
) -> bool {
    let id = Id::new("bae-ui-motion.custom-resolution");
    let shown = dialog::window(ctx, id, tr("Custom Resolution"), |ui| {
        ui.set_width(300.0);
        for (i, label) in [(0, "pixels horizontally."), (1, "pixels vertically.")] {
            ui.horizontal(|ui| {
                ui.label(tr("Render every"));
                let mut n = f64::from(steps[i]);
                if dialogs::fields::number(ui, &mut n, 1.0..=99.0, 0, "", tr(label)).changed() {
                    steps[i] = n.round().clamp(1.0, 99.0) as u32;
                }
                ui.label(tr(label));
            });
        }
        ui.add_space(4.0);
        dialog::buttons(ui, true)
    });
    match outcome(ctx, id, shown) {
        Outcome::None => true,
        Outcome::Cancel => false,
        Outcome::Ok => {
            let session = common::session(cx.resources);
            super::set_resolution(cx, &session, *steps, nested);
            false
        }
    }
}

/// Chinese names (AE's zh_CN dictionary: CompViewOptions, BackgroundColor,
/// CustomResolution).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("View Options", "视图选项"),
    ("Layer Controls", "图层控制"),
    ("Handles", "手柄"),
    ("Masks", "蒙版"),
    ("Mask Feather Boundaries", "蒙版羽化边界"),
    ("Effect Controls", "效果控件"),
    ("Keyframes", "关键帧"),
    ("Motion Paths", "运动路径"),
    ("Motion Path Tangents", "运动路径切线"),
    ("3D Transform Gizmo", "3D 变换小工具"),
    ("Camera Wireframes -", "摄像机线框 -"),
    ("Spot Light Wireframes -", "点光线框 -"),
    ("On", "开"),
    ("Off", "关"),
    ("When Selected", "选定时"),
    ("View", "视图"),
    ("Pixel Aspect Ratio Correction", "像素长宽比校正"),
    (
        "Select Custom Panel Background Color",
        "选择自定义面板背景颜色",
    ),
    ("Custom Resolution", "自定义分辨率"),
    (
        "Set Composition Resolution to Custom",
        "将合成分辨率设为自定义",
    ),
    (
        "Set Composition Resolution to Custom for the Focused and Nested Comps",
        "将“合成分辨率”设置为“自定义”，以便对嵌套在其中的焦点合成和预合成进行处理",
    ),
    ("Render every", "渲染每"),
    ("pixels horizontally.", "个像素（水平）。"),
    ("pixels vertically.", "个像素（垂直）。"),
];
