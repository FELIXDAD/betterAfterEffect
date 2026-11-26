use super::{Kind, Simulation};
use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use std::sync::Arc;

struct Control {
    id: u32,
    en: &'static str,
    zh: &'static str,
    kind: ControlKind,
    varies: bool,
}

#[derive(Clone, Copy)]
enum ControlKind {
    GroupStart,
    GroupEnd,
    Mask,
    Layer,
    Color([f64; 4]),
    Point([f64; 2]),
    Popup(i64, &'static [(&'static str, &'static str)]),
    Checkbox(bool),
    Angle(f64),
    Slider(f64, f64, f64, &'static str),
}

macro_rules! reference {
    ($id:literal, $en:literal, $zh:literal) => {
        (
            $id,
            $en,
            $zh,
            include!(concat!(env!("OUT_DIR"), "/", $id, ".rs")),
        )
    };
}

pub(super) fn builtin(kind: Kind) -> Builtin {
    let (id, name, zh, controls): (_, _, _, &[Control]) = match kind {
        Kind::World => reference!(
            "CC Particle World",
            "CC Particle World",
            "CC Particle World"
        ),
        Kind::Systems => reference!(
            "CC Particle Systems II",
            "CC Particle Systems II",
            "CC Particle Systems II"
        ),

        Kind::Playground => reference!("ADBE Playgnd", "Particle Playground", "粒子运动场"),
        Kind::Ball => reference!("CC Ball Action", "CC Ball Action", "CC Ball Action"),
        Kind::Mercury => reference!("CC Mr. Mercury", "CC Mr. Mercury", "CC Mr. Mercury"),
        Kind::Hair => reference!("CC Hair", "CC Hair", "CC Hair"),
        Kind::Rain => reference!("CSRainfall", "CC Rainfall", "CC Rainfall"),
        Kind::Snow => reference!("CSSnowfall", "CC Snowfall", "CC Snowfall"),
        Kind::Bubbles => reference!("CC Bubbles", "CC Bubbles", "CC Bubbles"),
        Kind::Drizzle => reference!("CC Drizzle", "CC Drizzle", "CC Drizzle"),
        Kind::Stars => reference!("CC Star Burst", "CC Star Burst", "CC Star Burst"),
        Kind::Scatter => reference!("CC Scatterize", "CC Scatterize", "CC Scatterize"),
        Kind::Polly => reference!("CC Pixel Polly", "CC Pixel Polly", "CC Pixel Polly"),
        Kind::Shatter => reference!("APC Shatter", "Shatter", "碎片"),
        Kind::Cards => reference!("APC CardDanceCam", "Card Dance", "卡片动画"),
    };
    let mut builder = DefBuilder::new(id, name, "Simulation");
    let mut localized = Vec::new();
    for c in controls {
        use ControlKind::*;
        let (kind, value) = match c.kind {
            GroupStart => (ParamKind::GroupStart, None),
            GroupEnd => (ParamKind::GroupEnd, None),
            Mask => (ParamKind::Path, None),
            Layer => (ParamKind::Layer, None),
            Color(v) => (ParamKind::Color, Some(Value::Color(v))),
            Point(v) => (ParamKind::Point, Some(Value::Vec2(v))),
            Popup(v, labels) => {
                localized.push((c.id, labels.iter().map(|&(_, zh)| zh.to_owned()).collect()));
                let options = labels.iter().map(|&(en, _)| en.to_owned()).collect();
                (ParamKind::Popup { options }, Some(Value::Int(v)))
            }
            Checkbox(v) => (ParamKind::Checkbox, Some(Value::Bool(v))),
            Angle(v) => (ParamKind::Angle, Some(Value::Float(v))),
            Slider(v, min, max, unit) => (
                ParamKind::Slider {
                    valid_min: min,
                    valid_max: max,
                    slider_min: min,
                    slider_max: max,
                    precision: 3,
                    integer: false,
                    display: unit.into(),
                },
                Some(Value::Float(v)),
            ),
        };
        let layer = matches!(kind, ParamKind::Layer);
        builder.push(c.id, c.en, c.zh, kind, value);
        if layer && matches!(id, "ADBE Playgnd" | "CC Hair") {
            builder = builder.layer_stage(c.id);
        }
    }
    let (mut def, names) = builder.reference_points().build();
    for (param, control) in def
        .params
        .iter_mut()
        .filter(|c| c.match_name.is_some())
        .zip(controls)
    {
        param.flags.cannot_animate = !control.varies;
        param.flags.cannot_time_vary = !control.varies;
    }
    let slug = name.to_ascii_lowercase().replace('.', "").replace(' ', "-");
    let mut result = rust_builtin(
        (Arc::new(Simulation { def, kind }), names),
        &slug,
        zh,
        &["simulation", "模拟", "粒子"],
    );
    result.info.param_options_zh = localized;
    result
}
