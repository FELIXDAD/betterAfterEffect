//! Keyframes in the layer-bar time graph: glyphs, selection and drags.

use super::*;

/// AE keyframe glyph: each half shows its side's interpolation.
pub(super) fn paint_key(p: &egui::Painter, c: Pos2, k: &Keyframe, selected: bool) {
    let fill = if selected {
        color::ACCENT
    } else {
        color::KEYFRAME
    };
    let stroke = Stroke::new(1.0, Color32::from_gray(20));
    let r = 5.0;
    let half = |left: bool, interp: Interpolation, auto: bool| -> Vec<Pos2> {
        let s = if left { -1.0 } else { 1.0 };
        match interp {
            Interpolation::Hold => vec![
                c + vec2(0.0, -r),
                c + vec2(s * r * 0.8, -r),
                c + vec2(s * r * 0.8, r),
                c + vec2(0.0, r),
            ],
            Interpolation::Bezier if auto => (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 8.0) - std::f32::consts::FRAC_PI_2;
                    c + vec2(s * a.cos() * r, a.sin() * r)
                })
                .collect(),
            Interpolation::Bezier => vec![
                c + vec2(0.0, -r),
                c + vec2(s * r * 0.75, -r),
                c + vec2(s * r * 0.2, 0.0),
                c + vec2(s * r * 0.75, r),
                c + vec2(0.0, r),
            ],
            Interpolation::Linear => {
                vec![c + vec2(0.0, -r), c + vec2(s * r, 0.0), c + vec2(0.0, r)]
            }
        }
    };
    let auto = k.flags.temporal_auto_bezier;
    for (left, interp) in [(true, k.interp_in), (false, k.interp_out)] {
        let pts = half(left, interp, auto);
        p.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
        p.add(Shape::line(pts, stroke));
    }
}
