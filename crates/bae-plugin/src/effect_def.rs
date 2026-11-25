//! Manifest effect declarations → `bae_model::EffectDef` (spec §5.3.6, §5.3.7).
//!
//! The document model owns effect *definitions* (`bae_model::EffectRegistry`,
//! shared with the UI, agents and the AE importer); this module derives them
//! from manifests so plugin effects and built-in effects are described by the
//! same type. Mapping:
//!
//! | manifest `type` | `bae_model::ParamKind` | stored value |
//! |---|---|---|
//! | `float` | `Slider { integer: false }` | `Float` |
//! | `int` | `Slider { integer: true }` | `Int` |
//! | `angle` | `Angle` | `Float` (degrees) |
//! | `checkbox` | `Checkbox` | `Bool` |
//! | `color` | `Color` | `Color` (straight RGBA) |
//! | `point` / `point3d` | `Point` / `Point3d` | `Vec2` / `Vec3` (layer pixels) |
//! | `popup` | `Popup` | `Int` (1-based index, as in AE) |
//! | `layer` | `Layer` | `PrimRef` |
//! | `group` | `GroupStart` … `GroupEnd` (`end_id`) | — |
//! | `button` | `Button` | — |
//! | `arbitrary` | `ArbitraryData` | `Bytes` |
//! | `path` | `Path` | `PrimRef` (mask) |
//!
//! `string` and `asset` are reserved until the document model has those
//! parameter kinds. Relative point defaults (a fraction of the layer size)
//! keep the fraction as the `EffectDef` default with
//! `ParamFlags::relative_default`; `effect.apply` scales it by the layer size
//! ([`relative_point_default`] does the same for a known size).

use bae_model::{EffectDef, EffectParamDef, ParamFlags, ParamKind, PluginRef, Value};

use crate::manifest::{
    EffectDecl, EffectImplementation, NumberDisplay, ParamDecl, ParamFlag, ParamType, PluginInfo,
    Threading,
};

/// Builds the effect definition registered in `bae_model::EffectRegistry`.
///
/// `match_name` is the full id `<plugin-id>/<effect-id>`. Call on validated
/// manifests only (invalid values are mapped best-effort).
pub fn effect_def(plugin: &PluginInfo, effect: &EffectDecl) -> EffectDef {
    let mut params = Vec::with_capacity(effect.params.len() + 2);
    let mut groups = Vec::<(&str, u32)>::new();
    for p in &effect.params {
        // Close the open group when this param is not one of its members.
        while let Some(&(key, end_id)) = groups.last()
            && p.group.as_deref() != Some(key)
        {
            params.push(group_end(end_id));
            groups.pop();
        }
        if let Some(def) = param_def(p) {
            params.push(def);
        }
        if p.ty == ParamType::Group {
            groups.push((&p.key, p.end_id.unwrap_or(0)));
        }
    }
    for (_, end_id) in groups.into_iter().rev() {
        params.push(group_end(end_id));
    }
    let wgsl = effect.implementation == EffectImplementation::Wgsl;
    EffectDef {
        match_name: crate::id::full_id(&plugin.id, &effect.id),
        name: effect.name.clone(),
        category: effect.category.clone(),
        plugin: Some(PluginRef {
            id: plugin.id.clone(),
            version: plugin.version.to_string(),
        }),
        params,
        thread_safe: wgsl || effect.threading == Some(Threading::Concurrent),
        gpu: wgsl || effect.devices.contains(&crate::manifest::Device::Gpu),
    }
}

fn group_end(id: u32) -> EffectParamDef {
    EffectParamDef {
        id,
        name: String::new(),
        match_name: None,
        kind: ParamKind::GroupEnd,
        default: None,
        flags: ParamFlags::default(),
    }
}

/// The definition of one manifest param; `None` for reserved types.
pub fn param_def(p: &ParamDecl) -> Option<EffectParamDef> {
    let [lo, hi] = p.valid_range();
    let [slo, shi] = p.slider.unwrap_or([lo, hi]);
    let display = p
        .display
        .unwrap_or(NumberDisplay::Number)
        .model_token()
        .to_owned();
    let nums = p.default_numbers();
    let (kind, default) = match p.ty {
        ParamType::Float => (
            ParamKind::Slider {
                valid_min: lo,
                valid_max: hi,
                slider_min: slo,
                slider_max: shi,
                precision: p.precision.unwrap_or(2),
                integer: false,
                display,
            },
            Some(Value::Float(nums[0])),
        ),
        ParamType::Int => (
            ParamKind::Slider {
                valid_min: lo,
                valid_max: hi,
                slider_min: slo,
                slider_max: shi,
                precision: 0,
                integer: true,
                display,
            },
            Some(Value::Int(nums[0] as i64)),
        ),
        ParamType::Angle => (ParamKind::Angle, Some(Value::Float(nums[0]))),
        ParamType::Checkbox => (ParamKind::Checkbox, Some(Value::Bool(nums[0] != 0.0))),
        ParamType::Color => (
            ParamKind::Color,
            Some(Value::Color([nums[0], nums[1], nums[2], nums[3]])),
        ),
        ParamType::Point => (ParamKind::Point, Some(Value::Vec2([nums[0], nums[1]]))),
        ParamType::Point3d => (
            ParamKind::Point3d,
            Some(Value::Vec3([nums[0], nums[1], nums[2]])),
        ),
        ParamType::Popup => (
            ParamKind::Popup {
                options: p.choices.clone().unwrap_or_default(),
            },
            Some(Value::Int(nums[0] as i64)),
        ),
        ParamType::Layer => (ParamKind::Layer, None),
        ParamType::Group => (ParamKind::GroupStart, None),
        ParamType::Button => (ParamKind::Button, None),
        ParamType::Arbitrary => (ParamKind::ArbitraryData, None),
        ParamType::Path => (ParamKind::Path, None),
        ParamType::String | ParamType::Asset => return None,
    };
    Some(EffectParamDef {
        id: p.id,
        name: p.name.clone(),
        match_name: None,
        kind,
        default,
        flags: ParamFlags {
            hidden: p.has_flag(ParamFlag::Hidden),
            disabled: p.has_flag(ParamFlag::Disabled),
            supervise: p.has_flag(ParamFlag::Supervise),
            cannot_animate: p.has_flag(ParamFlag::NotAnimatable),
            cannot_time_vary: p.has_flag(ParamFlag::CannotTimeVary),
            collapsed: p.has_flag(ParamFlag::StartCollapsed) || p.collapsed == Some(true),
            custom_ui: false,
            relative_default: p.is_relative_point(),
        },
    })
}

/// The default of a relative point param for a layer of `layer_size` pixels.
pub fn relative_point_default(p: &ParamDecl, layer_size: [f64; 2]) -> Value {
    let n = p.default_numbers();
    match p.ty {
        ParamType::Point3d => Value::Vec3([n[0] * layer_size[0], n[1] * layer_size[1], n[2]]),
        _ => Value::Vec2([n[0] * layer_size[0], n[1] * layer_size[1]]),
    }
}
