use super::{Kind, Simulation};
use crate::{Builtin, catalog::DefBuilder, rust_builtin};
use bae_model::{ParamKind, Value};
use serde_json::Value as Json;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

static ENGLISH: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    include_str!("names.tsv")
        .lines()
        .chain(include_str!("options.tsv").lines())
        .filter_map(|s| s.split_once('\t'))
        .collect()
});

fn english(label: &str) -> &str {
    ENGLISH.get(label).copied().unwrap_or(label)
}

macro_rules! reference {
    ($id:literal, $en:literal, $zh:literal) => {
        (
            $id,
            $en,
            $zh,
            include_str!(concat!(env!("OUT_DIR"), "/", $id, ".json")),
        )
    };
}

pub(super) fn builtin(kind: Kind) -> Builtin {
    let (id, name, zh, reference) = match kind {
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
    let mut fixed = Vec::new();
    for (id, label, ty, default, min, max, choices, unit, varies) in
        serde_json::from_str::<Vec<Control>>(reference).expect("checked-in simulation reference")
    {
        let en = english(&label);
        let value = default.as_f64().unwrap_or_default();
        let (kind, value) = match ty.as_str() {
            "NO_VALUE" if label.is_empty() => (ParamKind::GroupEnd, None),
            "NO_VALUE" => (ParamKind::GroupStart, None),
            "LAYER_INDEX" => (ParamKind::Layer, None),
            "MASK_INDEX" => (ParamKind::Path, None),
            "COLOR" => (
                ParamKind::Color,
                Some(Value::Color(serde_json::from_value(default).unwrap())),
            ),
            "TwoD_SPATIAL" => (
                ParamKind::Point,
                Some(Value::Vec2(serde_json::from_value(default).unwrap())),
            ),
            _ if choices.is_some() => {
                let options = choices.unwrap();
                let translated = options.iter().map(|s| english(s).to_owned()).collect();
                localized.push((id, options));
                (
                    ParamKind::Popup {
                        options: translated,
                    },
                    Some(Value::Int(value as i64)),
                )
            }
            _ if min == Some(0.)
                && max == Some(1.)
                && matches!(
                    en,
                    "Auto Focal Length"
                        | "White Tiles Fixed"
                        | "Invert Gradient"
                        | "Source Alpha Inheritance"
                        | "Position"
                        | "Radius"
                        | "Motion Path"
                        | "Grid"
                        | "Horizon"
                        | "Axis Box"
                        | "Align To Direction"
                        | "Composite With Original"
                        | "Stochastic Wiggle"
                ) =>
            {
                (ParamKind::Checkbox, Some(Value::Bool(value != 0.)))
            }
            _ if unit.as_deref() == Some("度") && min.is_none() => {
                (ParamKind::Angle, Some(Value::Float(value)))
            }
            _ => {
                let lo = min.unwrap_or(-1_000_000.);
                let hi = max.unwrap_or(1_000_000.);
                let display = match unit.as_deref() {
                    Some("像素") => "pixels",
                    Some("度") => "degrees",
                    _ => "none",
                };
                (
                    ParamKind::Slider {
                        valid_min: lo,
                        valid_max: hi,
                        slider_min: lo,
                        slider_max: hi,
                        precision: 3,
                        integer: false,
                        display: display.into(),
                    },
                    Some(Value::Float(value)),
                )
            }
        };
        if !varies {
            fixed.push(id);
        }
        builder.push(id, en, &label, kind, value);
    }
    let (mut def, names) = builder.build();
    for param in &mut def.params {
        param.flags.cannot_animate = fixed.contains(&param.id);
        param.flags.cannot_time_vary = param.flags.cannot_animate;
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

type Control = (
    u32,
    String,
    String,
    Json,
    Option<f64>,
    Option<f64>,
    Option<Vec<String>>,
    Option<String>,
    bool,
);
