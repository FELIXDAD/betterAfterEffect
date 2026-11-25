//! AE Stroke: a fixed grid of round brush dabs along each mask.
use bae_model::{ParamKind, Value};

use super::paint::{Mark, Paint};
use super::{Generator, Path, color, percent, ranges, slider, style, trim};
use crate::{Builtin, catalog::DefBuilder, params::Params};

pub(crate) fn stroke() -> Builtin {
    let mut def = DefBuilder::new("ADBE Stroke", "Stroke", "Generate");
    def.push(1, "Path", "路径", ParamKind::Path, None);
    def = def
        .static_param()
        .checkbox(10, "All Masks", "所有蒙版", false)
        .static_param()
        .checkbox(11, "Stroke Sequentially", "顺序描边", true)
        .static_param();
    def = brush(def, "Opacity", "不透明度");
    def = slider(def, 8, "Start", "起始", 0., [0., 100.], "none");
    def = slider(def, 9, "End", "结束", 100., [0., 100.], "none");
    def = slider(def, 6, "Spacing", "间距", 15., [0., 100.], "none");
    Generator::Stroke.builtin(
        style(def, 7, "Paint Style", "绘画样式", 1),
        "stroke",
        "描边",
    )
}

pub(super) fn brush(mut def: DefBuilder, opacity: &str, zh: &str) -> DefBuilder {
    def.push(
        2,
        "Color",
        "颜色",
        ParamKind::Color,
        Some(Value::Color([1.; 4])),
    );
    def = slider(def, 3, "Brush Size", "画笔大小", 2., [0., 200.], "pixels");
    def = percent(def, 4, "Brush Hardness", "画笔硬度", 0.75, 62259. / 65536.);
    percent(def, 5, opacity, zh, 1., 1.)
}

pub(super) fn paint(v: Params<'_>) -> Paint {
    let radius = v.f(3).clamp(0., 200.);
    let hardness = v.f(4) as f32;
    let rgb = color(v, 2);
    let mut paint = Paint::new(v.popup(7) as u32, 0, v.f(5).clamp(0., 1.) as f32);
    if radius <= std::f64::consts::FRAC_1_SQRT_2 || v.p.mask_paths.is_empty() {
        return paint;
    }
    let paths: Vec<_> = v.p.mask_paths.iter().map(Path::from_mask).collect();
    let step = radius * 2. * v.f(6).clamp(1., 100.) * 0.01;
    for (_, path, from, to) in ranges(&paths, trim(v, 8, 9), v.b(10) && v.b(11)) {
        let first = (from / step).floor() as usize;
        let last = (to / step).floor() as usize;
        for i in first..last.max(first + 1) {
            let center = path.at(i as f64 * step);
            paint
                .marks
                .push(Mark::dab(center, radius, rgb, hardness, 1.));
        }
    }
    paint
}
