//! AE Write-on samples the animated brush at a regular interval.
use bae_model::{ParamKind, Time, Value};
use bae_render::{EffectParams, GpuImage};
use glam::DVec2;

use super::paint::{Mark, Paint};
use super::{Generator, color, slider, stroke::brush, style};
use crate::{Builtin, catalog::DefBuilder, params::Params};

pub(crate) fn write_on() -> Builtin {
    let mut def = DefBuilder::new("ADBE Write-on", "Write-on", "Generate");
    def.push(
        1,
        "Brush Position",
        "画笔位置",
        ParamKind::Point,
        Some(Value::Vec2([320., 180.])),
    );
    def = brush(def, "Brush Opacity", "画笔不透明度");
    def = slider(
        def,
        6,
        "Stroke Length (secs)",
        "描边长度（秒）",
        0.,
        [0., 3000.],
        "none",
    );
    def = slider(
        def,
        7,
        "Brush Spacing (secs)",
        "画笔间距（秒）",
        655. / 65536.,
        [66. / 65536., 3000.],
        "none",
    );
    def = def
        .popup(
            8,
            "Paint Time Properties",
            "绘画时间属性",
            3,
            &["None", "Opacity", "Color"],
        )
        .popup(
            9,
            "Brush Time Properties",
            "画笔时间属性",
            1,
            &["None", "Size", "Hardness", "Size & Hardness"],
        );
    Generator::WriteOn.builtin(
        style(def, 10, "Paint Style", "绘画样式", 1).reference_points(),
        "write-on",
        "写入",
    )
}

pub(super) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    match id {
        8 => Some(&["无", "不透明度", "颜色"]),
        9 => Some(&["无", "大小", "硬度", "大小和硬度"]),
        _ => None,
    }
}

pub(super) fn sample_times(v: Params<'_>) -> Vec<Time> {
    let fixed_time = |seconds: f64| Time::from_seconds((seconds * 65536.).round() / 65536.);
    let duration = fixed_time(v.f(6).clamp(0., 3000.));
    let first = if duration == Time::ZERO {
        v.p.layer_in
    } else {
        v.p.layer_in.max(v.p.comp_time.saturating_sub(duration))
    };
    let elapsed = v.p.comp_time.saturating_sub(first).flicks();
    if elapsed < 0 {
        return Vec::new();
    }
    let step = fixed_time(v.f(7).clamp(66. / 65536., 3000.)).flicks();
    (0..=elapsed.saturating_sub(1).max(0) / step)
        .map(|i| first + Time::from_flicks(i * step))
        .collect()
}

pub(super) fn paint(v: Params<'_>, input: &GpuImage) -> Paint {
    let paint_mode = v.popup(8);
    let brush_mode = v.popup(9);
    let opacity = if paint_mode == 2 {
        1.
    } else {
        v.f(5).clamp(0., 1.) as f32
    };
    let mut paint = Paint::new(v.popup(10) as u32, 1, opacity);
    let r = if v.p.layer_bounds.is_empty() {
        input.rect
    } else {
        v.p.layer_bounds
    };
    let center = DVec2::new(
        f64::from(r.x0) + f64::from(r.width()) * 0.5,
        f64::from(r.y0) + f64::from(r.height()) * 0.5,
    );
    let mark = |p: &EffectParams<'_>| {
        let historical = Params::new(v.def, p);
        Mark::dab(
            p.point(1).map_or(center, DVec2::from_array),
            if matches!(brush_mode, 2 | 4) {
                historical.f(3)
            } else {
                v.f(3)
            }
            .clamp(0., 200.),
            color(if paint_mode == 3 { historical } else { v }, 2),
            if matches!(brush_mode, 3 | 4) {
                historical.f(4)
            } else {
                v.f(4)
            } as f32,
            if paint_mode == 2 {
                historical.f(5).clamp(0., 1.) as f32
            } else {
                1.
            },
        )
    };
    if v.p.history.is_empty() {
        paint.marks.push(mark(v.p));
    } else {
        for (_, sample) in v.p.history {
            let next = mark(&EffectParams::new(sample));
            if paint.marks.last().is_some_and(|prev| {
                prev.start == next.start
                    && prev.radius == next.radius
                    && prev.rgb == next.rgb
                    && prev.hardness == next.hardness
                    && prev.alpha == next.alpha
            }) {
                continue;
            }
            paint.marks.push(next);
        }
    }
    paint
}
