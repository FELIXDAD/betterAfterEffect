//! The Composition viewer's bottom control bar: magnification, resolution,
//! transparency grid, safe areas, grid, current time.

use bae_model::Time;
use bae_motion::query::CompInfo;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{tr, widgets};
use egui::{Rect, Ui, Vec2, vec2};

use super::{CompositionViewer, MAGNIFICATIONS};
use crate::common;
use crate::session::SharedSession;

pub(super) fn bottom_bar(
    ui: &mut Ui,
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

    // Resolution (used by the GPU preview once it lands).
    let res = [tr("Full"), tr("Half"), tr("Third"), tr("Quarter")];
    let mut r = (session.lock().resolution.clamp(1, 4) - 1) as usize;
    if widgets::dropdown(ui, "res", &mut r, &res, 84.0)
        .on_hover_text(tr("Resolution"))
        .changed()
    {
        session.lock().resolution = r as u32 + 1;
    }

    for (on, icon, tip) in [
        (&mut v.checker, Icon::TransparencyGrid, "Transparency Grid"),
        (&mut v.safe, Icon::RegionOfInterest, "Title/Action Safe"),
        (&mut v.grid, Icon::Guides, "Grid"),
    ] {
        if icons::button(ui, icon, 20.0, *on)
            .labeled_toggle(egui::WidgetType::Checkbox, tr(tip), *on)
            .on_hover_text(tr(tip))
            .clicked()
        {
            *on = !*on;
        }
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
