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
    // AE lists Custom… last; a custom resolution shows as Custom.
    let steps = super::comp_steps(cx.stage, &info.path);
    let custom = steps[0] != steps[1] || factor > 4;
    res.push(if custom {
        tr("Custom")
    } else {
        tr("Custom...")
    });
    let mut r = if session.lock().auto_resolution {
        0
    } else if custom {
        5
    } else {
        factor as usize
    };
    if widgets::dropdown(ui, "res", &mut r, &res, 84.0)
        .on_hover_text(tr("Resolution"))
        .changed()
    {
        match r {
            0 => session.lock().auto_resolution = true,
            1..=4 => super::set_resolution(cx, session, [r as u32; 2], false),
            _ => crate::dialogs::open(
                cx.resources,
                crate::dialogs::Dialog::CustomResolution {
                    steps,
                    nested: false,
                },
            ),
        }
    }

    let mut opts = session.lock().view_options;
    for (on, icon, tip) in [
        (&mut v.checker, Icon::TransparencyGrid, "Transparency Grid"),
        (&mut opts.safe, Icon::RegionOfInterest, "Title/Action Safe"),
        (&mut opts.grid, Icon::Guides, "Grid"),
    ] {
        if icons::button(ui, icon, 20.0, *on)
            .labeled_toggle(egui::WidgetType::Checkbox, tr(tip), *on)
            .on_hover_text(tr(tip))
            .clicked()
        {
            *on = !*on;
        }
    }
    session.lock().view_options = opts;
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
