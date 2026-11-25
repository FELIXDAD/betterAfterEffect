//! The Composition viewer's bottom control bar: magnification, resolution,
//! transparency grid, safe areas, grid, current time.

use bae_model::Time;
use bae_motion::query::CompInfo;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{PanelContext, tr, widgets};
use egui::{Rect, Ui, Vec2, vec2};

use super::{CompositionViewer, MAGNIFICATIONS};
use crate::common;
use crate::session::SharedSession;

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
) {
    ui.painter().rect_filled(bar, 0.0, color::PANEL);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(vec2(6.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 6.0;

    // Magnification.
    let mut labels: Vec<String> = vec![tr("Fit").to_owned()];
    labels.extend(
        MAGNIFICATIONS
            .iter()
            .map(|m| format!("{:.1}%", m * 100.0).replace(".0%", "%")),
    );
    let items: Vec<&str> = labels.iter().map(String::as_str).collect();
    let current = match v.zoom {
        None => 0,
        Some(z) => MAGNIFICATIONS
            .iter()
            .position(|m| (m - z).abs() < 1e-3)
            .map_or(usize::MAX, |i| i + 1),
    };
    let mut sel = current;
    let shown = if current == usize::MAX {
        format!("{:.1}%", mag * 100.0)
    } else {
        String::new()
    };
    if widgets::dropdown(ui, "mag", &mut sel, &items, 74.0)
        .on_hover_text(tr("Magnification"))
        .changed()
    {
        v.pan = Vec2::ZERO;
        v.zoom = if sel == 0 {
            None
        } else {
            Some(MAGNIFICATIONS[sel - 1])
        };
    }
    if !shown.is_empty() {
        ui.label(
            egui::RichText::new(shown)
                .font(theme::small())
                .color(color::TEXT),
        );
    }

    // Resolution: the comp's (as in Composition Settings), or Auto.
    let factor = super::comp_resolution(cx.stage, &info.path);
    let mut res = vec![
        tr("Auto"),
        tr("Full"),
        tr("Half"),
        tr("Third"),
        tr("Quarter"),
    ];
    if factor > 4 {
        res.push(tr("Custom"));
    }
    let mut r = if session.lock().auto_resolution {
        0
    } else {
        (factor as usize).min(5)
    };
    if widgets::dropdown(ui, "res", &mut r, &res, 84.0)
        .on_hover_text(tr("Resolution"))
        .changed()
    {
        match r {
            0 => session.lock().auto_resolution = true,
            1..=4 => super::set_resolution(cx, session, r as u32, false),
            _ => {}
        }
    }

    // AE hides the bar's controls that do not fit; the 3D View popup and the
    // time stay.
    let reserve = if v.on_gpu { 220.0 } else { 330.0 };
    let room = |ui: &Ui, w: f32| ui.available_width() >= w + reserve;
    // Choose grid and guide options (AE's popup).
    let mut opts = session.lock().view_options;
    let on = opts.safe || opts.grid || opts.proportional || opts.rulers;
    let resp = icons::button(ui, Icon::Guides, 20.0, on)
        .labeled(
            egui::WidgetType::Button,
            tr("Choose grid and guide options"),
        )
        .on_hover_text(tr("Choose grid and guide options"));
    common::popup_menu(&resp, &mut |ui| {
        for (flag, label) in [
            (&mut opts.safe, "Title/Action Safe"),
            (&mut opts.proportional, "Proportional Grid"),
            (&mut opts.grid, "Grid"),
            (&mut opts.guides, "Guides"),
            (&mut opts.rulers, "Rulers"),
        ] {
            if ui.checkbox(flag, tr(label)).clicked() {
                ui.close();
            }
        }
    });
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
        // Region of Interest (Alt/Option-click: draw a new one).
        let d = &mut v.display;
        let resp = icons::button(ui, Icon::RegionOfInterest, 20.0, d.roi_on)
            .labeled_toggle(
                egui::WidgetType::Checkbox,
                tr("Region of Interest"),
                d.roi_on,
            )
            .on_hover_text(tr("Region of Interest"));
        if resp.clicked() {
            if ui.input(|i| i.modifiers.alt) {
                d.roi = None;
                d.roi_on = true;
            } else {
                d.roi_on = !d.roi_on;
            }
        }
        if icons::button(ui, Icon::TransparencyGrid, 20.0, v.checker)
            .labeled_toggle(
                egui::WidgetType::Checkbox,
                tr("Transparency Grid"),
                v.checker,
            )
            .on_hover_text(tr("Transparency Grid"))
            .clicked()
        {
            v.checker = !v.checker;
        }
    }
    if room(ui, 24.0) {
        // Fast Previews.
        if room(ui, 24.0) {
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
            });
        }
        // Show Channel and Colorize.
        let d = &mut v.display;
        let resp = icons::button(ui, Icon::Channels, 20.0, d.channel != 0)
            .labeled(
                egui::WidgetType::Button,
                tr("Show Channel and Color Management Settings"),
            )
            .on_hover_text(tr("Show Channel and Color Management Settings"));
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
        });
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
    // 3D View popup.
    let views = super::view3d::View3d::ALL;
    let labels: Vec<&str> = views.iter().map(|v| tr(v.title())).collect();
    let current = session
        .lock()
        .views3d
        .get(&info.path)
        .map_or(super::view3d::View3d::Active, |v| v.current);
    let mut i = views.iter().position(|v| *v == current).unwrap_or(0);
    if widgets::dropdown(ui, "view3d", &mut i, &labels, 100.0)
        .on_hover_text(tr("3D View Popup"))
        .changed()
    {
        let mut s = session.lock();
        s.views3d
            .entry(info.path.clone())
            .or_default()
            .switch(views[i]);
    }
    ui.label(
        egui::RichText::new(common::timecode(info, time))
            .font(theme::body())
            .color(color::ACCENT),
    );
    if !v.on_gpu {
        ui.label(
            egui::RichText::new(tr("Preview (simplified)"))
                .font(theme::small())
                .color(color::TEXT_DIM),
        );
    }
}
