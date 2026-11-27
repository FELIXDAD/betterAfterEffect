//! The Composition viewer's bottom control bar, with AE 26's popups and
//! tooltips: magnification (Fit, Fit up to 100%, the steps; the fitted value
//! shows in parentheses), resolution (Full … Quarter, Custom…, Auto), grid
//! and guide options (Alt/Option-click: Title/Action Safe), mask and shape
//! path visibility, region of interest (Alt/Option-click: reset),
//! transparency grid, fast previews, show channel (Alt/Option-click: Alpha),
//! snapshots, exposure, the 3D view and view layout popups, and the preview
//! time (click: Go to Time).

use bae_model::Time;
use bae_motion::query::CompInfo;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{PanelContext, tr};
use egui::{Align2, Rect, Sense, Ui, Vec2, pos2, vec2};

use super::view3d::View3d;
use super::{CompositionViewer, MAGNIFICATIONS};
use crate::common;
use crate::session::SharedSession;

/// An AE tooltip: the control's name and, on a second line, what an
/// Alt-click (Option-click on the Mac) does.
fn tip(name: &str, mac: &str, other: &str) -> String {
    let alt = if cfg!(target_os = "macos") {
        mac
    } else {
        other
    };
    format!("{}\n{}", tr(name), tr(alt))
}

/// A popup field as AE draws it (the value and a chevron); the caller adds
/// the menu.
fn field(ui: &mut Ui, text: &str, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 17.0), Sense::click());
    let resp = resp.labeled_value(egui::WidgetType::ComboBox, text, text);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if resp.hovered() {
            color::FIELD_HOVER
        } else {
            color::DROPDOWN
        };
        p.rect_filled(rect, 3.0, bg);
        p.with_clip_rect(rect.shrink2(vec2(2.0, 0.0))).text(
            pos2(rect.left() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            theme::body(),
            color::TEXT_STRONG,
        );
        let chev = Rect::from_center_size(
            pos2(rect.right() - 11.0, rect.center().y),
            Vec2::splat(14.0),
        );
        icons::paint(p, chev, Icon::ChevronDown, color::TEXT_STRONG);
    }
    resp
}

/// A magnification as AE writes it: a step's own label (6.25%), else one
/// decimal when it has one (33.3%).
fn percent(m: f32) -> String {
    if let Some((_, label)) = MAGNIFICATIONS.iter().find(|(s, _)| (s - m).abs() < 1e-3) {
        return (*label).to_owned();
    }
    let p = m * 100.0;
    if (p - p.round()).abs() < 0.05 {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%")
    }
}

/// The 3D View popup's line for Active Camera: `Active Camera (Camera 1)`,
/// or `Active Camera (Default)` without a camera layer.
fn active_title(camera: Option<&str>) -> String {
    camera.map_or_else(
        || tr("Active Camera (Default)").to_owned(),
        |name| tr("Active Camera (@0)").replace("@0", name),
    )
}

/// The popup's reset line: AE's `Reset Camera 1` for the active camera layer
/// (`Reset Hero Camera` when its name lacks "Camera"), else `Reset 3D View`.
fn reset_title(current: View3d, camera: Option<&str>) -> String {
    match (current, camera) {
        (View3d::Active, Some(name)) if name.contains(tr("Camera")) || name.contains("Camera") => {
            tr("Reset @0").replace("@0", name)
        }
        (View3d::Active, Some(name)) => tr("Reset @0 Camera").replace("@0", name),
        _ => tr("Reset 3D View").to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bottom_bar(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    bar: Rect,
    v: &mut CompositionViewer,
    info: &CompInfo,
    time: Time,
    mag: f32,
    session: &SharedSession,
    camera: Option<&str>,
) {
    ui.painter().rect_filled(bar, 0.0, color::PANEL);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(vec2(6.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 6.0;
    let alt = ui.input(|i| i.modifiers.alt);

    // Magnification: a step, or fitted (shown in parentheses).
    let shown = match v.zoom {
        None => format!("({})", percent(mag)),
        Some(z) => percent(z),
    };
    let resp = field(ui, &shown, 74.0).on_hover_text(tr("Magnification percentage popup"));
    common::popup_menu(&resp, &mut |ui| {
        for (all, label) in [(true, "Fit"), (false, "Fit up to 100%")] {
            if ui
                .radio(v.zoom.is_none() && v.fit_all == all, tr(label))
                .clicked()
            {
                v.zoom = None;
                v.fit_all = all;
                v.pan = Vec2::ZERO;
                ui.close();
            }
        }
        ui.separator();
        for (m, label) in MAGNIFICATIONS {
            if ui
                .radio(v.zoom.is_some_and(|z| (z - m).abs() < 1e-3), label)
                .clicked()
            {
                v.zoom = Some(m);
                v.pan = Vec2::ZERO;
                ui.close();
            }
        }
    });

    // Resolution: the comp's (as in Composition Settings), or Auto.
    let factor = super::comp_resolution(cx.stage, &info.path);
    let auto = session.lock().auto_resolution;
    // AE lists Custom… last; a custom resolution shows as Custom.
    let steps = super::comp_steps(cx.stage, &info.path);
    let custom = steps[0] != steps[1] || factor > 4;
    let names = ["Full", "Half", "Third", "Quarter"];
    let shown = if auto {
        tr("Auto")
    } else if custom {
        tr("Custom")
    } else {
        names
            .get(factor as usize - 1)
            .map_or_else(|| tr("Custom"), |n| tr(n))
    };
    let resp = field(ui, shown, 84.0).on_hover_text(tr("Resolution/Down Sample Factor Popup"));
    common::popup_menu(&resp, &mut |ui| {
        for (i, name) in names.iter().enumerate() {
            let step = i as u32 + 1;
            if ui
                .radio(!auto && !custom && factor == step, tr(name))
                .clicked()
            {
                super::set_resolution(cx, session, [step; 2], false);
                ui.close();
            }
        }
        ui.separator();
        if ui.radio(!auto && custom, tr("Custom...")).clicked() {
            crate::dialogs::open(
                cx.resources,
                crate::dialogs::Dialog::CustomResolution {
                    steps,
                    nested: false,
                },
            );
            ui.close();
        }
        if ui.radio(auto, tr("Auto")).clicked() {
            session.lock().auto_resolution = true;
            ui.close();
        }
    });

    // AE hides the bar's controls that do not fit; the 3D View popup, the
    // view layout popup and the time stay.
    let reserve = if v.on_gpu { 300.0 } else { 330.0 };
    let room = |ui: &Ui, w: f32| ui.available_width() >= w + reserve;
    // Choose grid and guide options (Alt/Option-click: Title/Action Safe).
    let mut opts = session.lock().view_options;
    let on = opts.safe || opts.grid || opts.proportional || opts.rulers;
    let resp = icons::button(ui, Icon::Guides, 20.0, on)
        .labeled(
            egui::WidgetType::Button,
            tr("Choose grid and guide options"),
        )
        .on_hover_text(tip(
            "Choose grid and guide options",
            "Option-click to toggle Title/Action Safe",
            "Alt-click to toggle Title/Action Safe",
        ));
    if alt {
        if resp.clicked() {
            opts.safe = !opts.safe;
        }
    } else {
        common::popup_menu(&resp, &mut |ui| {
            for (flag, label) in [
                (&mut opts.safe, "Title/Action Safe"),
                (&mut opts.proportional, "Proportional Grid"),
                (&mut opts.grid, "Grid"),
                (&mut opts.guides, "Guides"),
                (&mut opts.rulers, "Rulers"),
                (&mut opts.axes, "3D Reference Axes"),
            ] {
                if ui.checkbox(flag, tr(label)).clicked() {
                    ui.close();
                }
            }
        });
    }
    // Toggle Mask and Shape Path Visibility.
    if room(ui, 24.0)
        && icons::button(ui, Icon::MaskVisibility, 20.0, opts.paths)
            .labeled_toggle(
                egui::WidgetType::Checkbox,
                tr("Toggle Mask and Shape Path Visibility"),
                opts.paths,
            )
            .on_hover_text(tr("Toggle Mask and Shape Path Visibility"))
            .clicked()
    {
        opts.paths = !opts.paths;
    }
    session.lock().view_options = opts;
    if room(ui, 48.0) {
        // Region of Interest (Alt/Option-click: reset, draw a new one).
        let d = &mut v.display;
        let resp = icons::button(ui, Icon::RegionOfInterest, 20.0, d.roi_on)
            .labeled_toggle(
                egui::WidgetType::Checkbox,
                tr("Region of Interest"),
                d.roi_on,
            )
            .on_hover_text(tip(
                "Region of Interest",
                "Option-click to reset",
                "Alt-click to reset",
            ));
        if resp.clicked() {
            if alt {
                d.roi = None;
                d.roi_on = true;
            } else {
                d.roi_on = !d.roi_on;
            }
        }
        if icons::button(ui, Icon::TransparencyGrid, 20.0, v.checker)
            .labeled_toggle(
                egui::WidgetType::Checkbox,
                tr("Toggle Transparency Grid"),
                v.checker,
            )
            .on_hover_text(tr("Toggle Transparency Grid"))
            .clicked()
        {
            v.checker = !v.checker;
        }
    }
    if room(ui, 48.0) {
        // Fast Previews.
        let d = &mut v.display;
        let resp = icons::button(
            ui,
            Icon::FastPreview,
            20.0,
            d.fast != super::display::Fast::Off,
        )
        .labeled(egui::WidgetType::Button, tr("Fast Previews"))
        .on_hover_text(tr("Fast Previews"));
        common::popup_menu(&resp, &mut |ui| {
            for (f, name) in super::display::Fast::ALL {
                if ui.radio(d.fast == f, tr(name)).clicked() {
                    d.fast = f;
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .button(tr("Adaptive Resolution Preferences..."))
                .clicked()
            {
                cx.invoke_with(
                    bae_ui::settings::ACTION,
                    serde_json::json!({"page": "previews"}),
                );
                ui.close();
            }
        });
        // Show Channel and Color Management Settings (Alt/Option-click:
        // Alpha).
        let d = &mut v.display;
        let resp = icons::button(ui, Icon::Channels, 20.0, d.channel != 0)
            .labeled(
                egui::WidgetType::Button,
                tr("Show Channel and Color Management Settings"),
            )
            .on_hover_text(tip(
                "Show Channel and Color Management Settings",
                "Option-click to show Alpha",
                "Alt-click to show Alpha",
            ));
        if alt {
            if resp.clicked() {
                d.channel = if d.channel == ALPHA { 0 } else { ALPHA };
            }
        } else {
            common::popup_menu(&resp, &mut |ui| {
                for (i, name) in super::display::CHANNELS.iter().enumerate() {
                    if ui.radio(d.channel == i as u32, tr(name)).clicked() {
                        d.channel = i as u32;
                        ui.close();
                    }
                }
                ui.separator();
                if ui.checkbox(&mut d.colorize, tr("Colorize")).clicked() {
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Set Project Working Space...")).clicked() {
                    crate::dialogs::project_settings(cx);
                    ui.close();
                }
            });
        }
    }
    if room(ui, 48.0) {
        // Take Snapshot / Show Snapshot (held).
        if icons::button(ui, Icon::Snapshot, 20.0, false)
            .labeled(egui::WidgetType::Button, tr("Take Snapshot"))
            .on_hover_text(tr("Take Snapshot"))
            .clicked()
            && let Some(g) = v.gpu.as_mut()
        {
            g.take_snapshot(0);
        }
        let held = icons::button(ui, Icon::ShowSnapshot, 20.0, false)
            .labeled(egui::WidgetType::Button, tr("Show Snapshot"))
            .on_hover_text(tr("Show Snapshot"))
            .is_pointer_button_down_on();
        v.snapshot_button = held;
        if held {
            v.snapshot_shown = Some(0);
        }
    }
    // Exposure: the button resets it.
    if room(ui, 80.0) {
        let d = &mut v.display;
        if icons::button(ui, Icon::Exposure, 20.0, d.exposure != 0.0)
            .labeled(egui::WidgetType::Button, tr("Reset Exposure"))
            .on_hover_text(tr("Reset Exposure"))
            .clicked()
        {
            d.exposure = 0.0;
        }
        ui.add(
            egui::DragValue::new(&mut d.exposure)
                .speed(0.05)
                .range(-40.0..=40.0)
                .fixed_decimals(1),
        )
        .labeled(egui::WidgetType::DragValue, tr("Adjust Exposure"))
        .on_hover_text(tr("Adjust Exposure"));
    }
    // 3D View popup: Active Camera, Default, the six orthographic views, the
    // custom views, then the reset line (AE's View ▸ Switch 3D View).
    let current = session
        .lock()
        .views3d
        .get(&info.path)
        .map_or(View3d::Active, |v| v.current);
    let resp = field(ui, tr(current.title()), 100.0).on_hover_text(tr("3D View Popup"));
    common::popup_menu(&resp, &mut |ui| {
        for (i, view) in View3d::ALL.into_iter().enumerate() {
            // Separators after Active Camera, Default and Bottom.
            if matches!(i, 1 | 2 | 8) {
                ui.separator();
            }
            let title = match view {
                View3d::Active => active_title(camera),
                other => tr(other.title()).to_owned(),
            };
            if ui.radio(current == view, title).clicked() {
                cx.invoke_with(
                    super::view3d::ids::SWITCH,
                    serde_json::json!({ "view": view.key() }),
                );
                ui.close();
            }
        }
        ui.separator();
        if ui.button(reset_title(current, camera)).clicked() {
            cx.invoke(super::view3d::ids::RESET);
            ui.close();
        }
    });
    // Select view layout (AE hides bar controls that do not fit).
    if ui.available_width() > 170.0 {
        let n = session
            .lock()
            .views3d
            .get(&info.path)
            .map_or(1, super::view3d::Views::layout);
        let layouts = [(1, "1 View"), (2, "2 Views"), (4, "4 Views")];
        let shown = layouts
            .iter()
            .find(|(k, _)| *k == n)
            .map_or("1 View", |(_, t)| t);
        let resp = field(ui, tr(shown), 70.0).on_hover_text(tr("Select view layout"));
        common::popup_menu(&resp, &mut |ui| {
            for (k, title) in layouts {
                if ui.radio(n == k, tr(title)).clicked() {
                    cx.invoke_with(
                        super::view3d::ids::LAYOUT,
                        serde_json::json!({ "views": k }),
                    );
                    ui.close();
                }
            }
            ui.separator();
            let mut shared = super::view3d::shares_options(cx.resources);
            if ui.checkbox(&mut shared, tr("Share View Options")).clicked() {
                cx.invoke(super::view3d::ids::SHARE);
                ui.close();
            }
        });
    }
    // Preview time: a click opens Go to Time.
    if ui
        .add(
            egui::Label::new(
                egui::RichText::new(common::timecode(info, time))
                    .font(theme::body())
                    .color(color::ACCENT),
            )
            .sense(Sense::click()),
        )
        .on_hover_text(tr("Preview Time (Click to change Current Time)"))
        .clicked()
    {
        crate::dialogs::go_to_time(cx, session);
    }
    // BAE's own hint (no GPU): only when it fits.
    if !v.on_gpu && ui.available_width() > 120.0 {
        ui.label(
            egui::RichText::new(tr("Preview (simplified)"))
                .font(theme::small())
                .color(color::TEXT_DIM),
        );
    }
}

/// Index of Alpha in [`super::display::CHANNELS`].
const ALPHA: u32 = 4;
