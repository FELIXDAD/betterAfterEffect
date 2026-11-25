//! The Info panel (AE: Window > Info, Cmd/Ctrl+2): the color of the pixel under
//! the pointer in the Composition panel (R, G, B, A with a swatch), its X / Y in
//! comp pixels, and below a line, what AE reports there: the selected layer's
//! name, duration and In / Out, or while a layer is moved its position and how
//! far it moved. The panel menu picks how colors read (AE's eight modes).

use bae_model::{PrimPath, Stage};
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::common;

/// What the viewer reports to the Info panel (in the shared session).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readout {
    /// The pointer in comp pixels (kept when the pointer leaves the viewer, as AE).
    pub pos: Option<[f64; 2]>,
    /// Straight 8-bit RGBA of the pixel under the pointer (`None`: unknown).
    pub color: Option<[u8; 4]>,
    /// A layer being moved: (layer, position, offset since the drag began).
    pub moved: Option<(PrimPath, [f64; 2], [f64; 2])>,
    /// The Info panel drew in this frame number (the viewer reads pixels only
    /// while someone shows them).
    pub shown_at: u64,
}

/// How colors read (the panel menu, AE's order).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    /// The project's color depth (8 bpc in BAE).
    #[default]
    Auto,
    Percent,
    Web,
    Hsb,
    Bpc8,
    Bpc10,
    Bpc16,
    Decimal,
}

impl ColorMode {
    const ALL: [(ColorMode, &'static str); 8] = [
        (ColorMode::Auto, "Auto Color Display"),
        (ColorMode::Percent, "Percent"),
        (ColorMode::Web, "Web"),
        (ColorMode::Hsb, "HSB"),
        (ColorMode::Bpc8, "8-bpc (0-255)"),
        (ColorMode::Bpc10, "10-bpc (0-1023)"),
        (ColorMode::Bpc16, "16-bpc (0-32768)"),
        (ColorMode::Decimal, "Decimal (0.0-1.0)"),
    ];
}

/// The four labelled values of a color in `mode`. AE leaves R, G, B empty when
/// the pixel is fully transparent.
pub fn color_rows(mode: ColorMode, c: Option<[u8; 4]>) -> [(&'static str, String); 4] {
    let labels = if mode == ColorMode::Hsb {
        ["H", "S", "B", "A"]
    } else {
        ["R", "G", "B", "A"]
    };
    let Some(c) = c else {
        return labels.map(|l| (l, String::new()));
    };
    let unit = |v: u8| f64::from(v) / 255.0;
    let fmt = |v: u8| match mode {
        ColorMode::Auto | ColorMode::Bpc8 => v.to_string(),
        ColorMode::Bpc10 => format!("{}", (unit(v) * 1023.0).round()),
        ColorMode::Bpc16 => format!("{}", (unit(v) * 32768.0).round()),
        ColorMode::Decimal => format!("{:.3}", unit(v)),
        ColorMode::Percent | ColorMode::Hsb => format!("{}%", (unit(v) * 100.0).round()),
        ColorMode::Web => format!("{v:02X}"),
    };
    let mut values = [fmt(c[0]), fmt(c[1]), fmt(c[2]), fmt(c[3])];
    if mode == ColorMode::Hsb {
        let (h, s, b) = hsb(c);
        values[0] = format!("{}°", h.round());
        values[1] = format!("{}%", (s * 100.0).round());
        values[2] = format!("{}%", (b * 100.0).round());
    }
    if c[3] == 0 {
        for v in &mut values[..3] {
            v.clear();
        }
    }
    let [a, b, c2, d] = values;
    [
        (labels[0], a),
        (labels[1], b),
        (labels[2], c2),
        (labels[3], d),
    ]
}

/// Hue (degrees), saturation and brightness (0–1) of an 8-bit color.
fn hsb(c: [u8; 4]) -> (f64, f64, f64) {
    let [r, g, b] = [c[0], c[1], c[2]].map(|v| f64::from(v) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// The lines under the readout: a layer moved in the viewer (name, Position, Δ),
/// else the one selected layer (name, Duration, In / Out).
pub fn message_lines(stage: &Stage, session: &crate::session::Session) -> Vec<String> {
    let sep = tr(", ");
    if let Some((layer, p, d)) = &session.info.moved
        && session.selected_layers.contains(layer)
    {
        return vec![
            bae_motion::query::display_name_of(stage, layer),
            format!("{} {:.1}{sep}{:.1}", tr("Position:"), p[0], p[1]),
            format!("{} {:.1}{sep}{:.1}", tr("\u{394}:"), d[0], d[1]),
        ];
    }
    let [layer] = session.selected_layers.as_slice() else {
        return Vec::new();
    };
    let Some(comp) = bae_motion::query::owning_comp(stage, layer)
        .and_then(|c| bae_motion::query::comp_info(stage, &c))
    else {
        return Vec::new();
    };
    let t = bae_motion::query::layer_timing(stage, layer);
    let last = (t.out_point - comp.frame_duration()).max(t.in_point);
    vec![
        bae_motion::query::display_name_of(stage, layer),
        tr("Duration: {d}").replace(
            "{d}",
            &common::duration_code(&comp, t.out_point - t.in_point),
        ),
        tr("In: {in}, Out: {out}")
            .replace("{in}", &common::timecode(&comp, t.in_point))
            .replace("{out}", &common::timecode(&comp, last)),
    ]
}

/// The Info panel.
#[derive(Default)]
pub struct InfoPanel {
    mode: ColorMode,
}

impl InfoPanel {
    pub fn from_state(state: Option<&serde_json::Value>) -> Self {
        InfoPanel {
            mode: state
                .and_then(|s| s.get("mode"))
                .and_then(|m| serde_json::from_value(m.clone()).ok())
                .unwrap_or_default(),
        }
    }
}

/// Line height of the panel's text.
const LINE: f32 = 13.0;

impl Panel for InfoPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let (readout, lines) = {
            let mut s = session.lock();
            s.info.shown_at = ui.ctx().cumulative_frame_nr();
            (s.info.clone(), message_lines(cx.stage, &s))
        };
        let rows = color_rows(self.mode, readout.color);
        let font = bae_ui::theme::body();
        let text = bae_ui::theme::color::TEXT;
        let top = ui.max_rect().left_top() + vec2(8.0, 6.0);
        let p = ui.painter();
        // Swatch, then R G B A (AE: label, colon, value) and X / Y with the
        // crosshair under them.
        let swatch = Rect::from_min_size(top + vec2(0.0, LINE * 1.5), vec2(17.0, 17.0));
        let fill = readout.color.map_or(Color32::BLACK, |c| {
            let a = f32::from(c[3]) / 255.0;
            let k = |v: u8| (f32::from(v) * a) as u8;
            Color32::from_rgb(k(c[0]), k(c[1]), k(c[2]))
        });
        p.rect_filled(swatch, 0.0, fill);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = top.y + i as f32 * LINE + LINE / 2.0;
            p.text(
                pos2(top.x + 32.0, y),
                Align2::RIGHT_CENTER,
                label,
                font.clone(),
                text,
            );
            p.text(
                pos2(top.x + 35.0, y),
                Align2::LEFT_CENTER,
                ":",
                font.clone(),
                text,
            );
            p.text(
                pos2(top.x + 44.0, y),
                Align2::LEFT_CENTER,
                value,
                font.clone(),
                text,
            );
        }
        let x0 = top.x + 118.0;
        let (px, py) = readout
            .pos
            .map_or((String::new(), String::new()), |[x, y]| {
                (format!("{}", x.floor()), format!("{}", y.floor()))
            });
        for (i, (label, value)) in [("X", px), ("Y", py)].iter().enumerate() {
            let y = top.y + i as f32 * LINE + LINE / 2.0;
            p.text(pos2(x0, y), Align2::RIGHT_CENTER, label, font.clone(), text);
            p.text(
                pos2(x0 + 3.0, y),
                Align2::LEFT_CENTER,
                ":",
                font.clone(),
                text,
            );
            p.text(
                pos2(x0 + 12.0, y),
                Align2::LEFT_CENTER,
                value,
                font.clone(),
                text,
            );
        }
        let cross = pos2(x0 - 5.0, top.y + LINE * 2.6);
        let stroke = Stroke::new(1.0, text);
        p.line_segment([cross - vec2(4.0, 0.0), cross + vec2(4.0, 0.0)], stroke);
        p.line_segment([cross - vec2(0.0, 4.0), cross + vec2(0.0, 4.0)], stroke);
        // The line, then AE's messages.
        let rule = top.y + LINE * 4.0 + 8.0;
        let right = ui.max_rect().right() - 8.0;
        p.hline(
            top.x..=right,
            rule,
            Stroke::new(1.0, bae_ui::theme::color::DIVIDER),
        );
        for (i, l) in lines.iter().enumerate() {
            let y = rule + 6.0 + i as f32 * LINE + LINE / 2.0;
            p.text(
                pos2(top.x - 2.0, y),
                Align2::LEFT_CENTER,
                l,
                font.clone(),
                text,
            );
        }
        // Accessible values (screen readers, tests): one label per row.
        let height = rule + 8.0 + lines.len() as f32 * LINE - ui.max_rect().top();
        let (rect, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let mut summary: Vec<String> = rows.iter().map(|(l, v)| format!("{l}: {v}")).collect();
        summary.extend([
            format!(
                "X: {}",
                readout
                    .pos
                    .map_or(String::new(), |p| p[0].floor().to_string())
            ),
            format!(
                "Y: {}",
                readout
                    .pos
                    .map_or(String::new(), |p| p[1].floor().to_string())
            ),
        ]);
        summary.extend(lines);
        let _ = rect;
        use bae_ui::a11y::Accessible as _;
        resp.labeled_value(egui::WidgetType::Label, tr("Info"), &summary.join("\n"));
    }

    fn menu(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        for (mode, title) in ColorMode::ALL {
            if radio_item(ui, tr(title), self.mode == mode).clicked() {
                self.mode = mode;
                ui.close();
            }
        }
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "mode": self.mode }))
    }
}

/// A menu item with AE's radio dot.
fn radio_item(ui: &mut Ui, text: &str, on: bool) -> egui::Response {
    let width = ui.available_width().max(150.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 2.0, bae_ui::theme::color::ACCENT_DIM);
    }
    if on {
        p.circle_filled(
            pos2(rect.left() + 9.0, rect.center().y),
            2.5,
            bae_ui::theme::color::TEXT_STRONG,
        );
    }
    p.text(
        pos2(rect.left() + 20.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        bae_ui::theme::body(),
        bae_ui::theme::color::TEXT_STRONG,
    );
    use bae_ui::a11y::Accessible as _;
    resp.labeled_toggle(egui::WidgetType::RadioButton, text, on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_read_like_ae() {
        let red = Some([204, 51, 26, 255]);
        let v = |mode| color_rows(mode, red).map(|(_, v)| v);
        assert_eq!(v(ColorMode::Auto), ["204", "51", "26", "255"]);
        assert_eq!(v(ColorMode::Web), ["CC", "33", "1A", "FF"]);
        assert_eq!(v(ColorMode::Percent), ["80%", "20%", "10%", "100%"]);
        assert_eq!(v(ColorMode::Decimal)[0], "0.800");
        assert_eq!(v(ColorMode::Bpc10)[0], "818");
        let hsb = color_rows(ColorMode::Hsb, red);
        assert_eq!(hsb[0], ("H", "8°".to_owned()));
        // Transparent: R G B empty, A 0 (AE).
        let clear = color_rows(ColorMode::Auto, Some([0, 0, 0, 0])).map(|(_, v)| v);
        assert_eq!(clear, ["", "", "", "0"]);
        assert!(
            color_rows(ColorMode::Auto, None)
                .iter()
                .all(|(_, v)| v.is_empty())
        );
    }
}
