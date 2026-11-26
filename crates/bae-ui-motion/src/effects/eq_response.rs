use bae_ui::theme::{self, color};
use egui::{Align2, Sense, Stroke, Ui, pos2, vec2};

/// Magnitude response of the enabled bands on a logarithmic frequency axis.
pub(super) fn show(ui: &mut Ui, rate: u32, mut value: impl FnMut(u32) -> f64) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(360.0), 84.0), Sense::hover());
    if !ui.is_rect_visible(rect) || rect.width() < 48.0 {
        return;
    }
    let mut bands = Vec::with_capacity(3);
    for id in [2, 6, 10] {
        if value(id) >= 0.5 {
            bands.push((value(id + 1), value(id + 2), value(id + 3)));
        }
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, color::FIELD);
    let hi = (f64::from(rate) * 0.49).clamp(21.0, 20_000.0);
    let db: [f32; 128] = std::array::from_fn(|i| {
        let hz = 20.0 * (hi / 20.0).powf(i as f64 / 127.0);
        bands
            .iter()
            .map(|&(center, width, boost)| {
                bae_media::eq_band_response_db(hz, center, width, boost, rate)
            })
            .sum::<f64>() as f32
    });
    let scale = db.iter().fold(24.0_f32, |peak, x| peak.max(x.abs()));
    let plot = rect.shrink2(vec2(3.0, 15.0));
    painter.hline(
        plot.x_range(),
        plot.center().y,
        Stroke::new(1.0, color::FIELD_BORDER),
    );
    let points = db
        .iter()
        .enumerate()
        .map(|(i, &y)| {
            pos2(
                plot.left() + plot.width() * i as f32 / 127.0,
                plot.center().y - 0.5 * plot.height() * y / scale,
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, color::ACCENT)));
    painter.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        format!("±{scale:.0} dB"),
        theme::body(),
        color::TEXT,
    );
    painter.text(
        rect.left_bottom(),
        Align2::LEFT_BOTTOM,
        "20 Hz",
        theme::body(),
        color::TEXT,
    );
    painter.text(
        rect.right_bottom(),
        Align2::RIGHT_BOTTOM,
        format!("{:.1} kHz", hi / 1000.0),
        theme::body(),
        color::TEXT,
    );
}
