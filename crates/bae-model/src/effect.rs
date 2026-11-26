//! Effect definitions and parameters (ARCHITECTURE §15).
//!
//! Parameter types are a superset of AE's (`PF_Param_*`), every parameter has a stable
//! numeric ID that matches AE's parameter index — the number at the end of a parameter
//! matchName (`ADBE Gaussian Blur 2-0001` → 1) — so AE projects and plugins map 1:1.
//! Parameter values are ordinary attributes named `param:NNNN` on the `Effect` prim, so
//! they animate, compose, sync and undo like everything else; an effect instance also
//! has an opaque `state` slot (AE sequence data).
//!
//! Effects of plugins that are not installed keep every attribute untouched and are
//! reported as placeholders ([`EffectStatus`]) until the plugin comes back or the
//! project goes back to AE.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::compose::PrimView;
use crate::prim::PrimType;
use crate::value::{Value, ValueType};

/// Attribute name of parameter `id` (`param:0001`).
pub fn param_attr(id: u32) -> String {
    format!("param:{id:04}")
}

/// Parameter ID of a `param:NNNN` attribute.
pub fn param_id(attr: &str) -> Option<u32> {
    attr.strip_prefix("param:")?.parse().ok()
}

/// Parameter kinds (AE `PF_Param_*` superset).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamKind {
    /// Numeric slider (AE slider and float slider). `valid_*` bound accepted values,
    /// `slider_*` the visible slider range.
    Slider {
        valid_min: f64,
        valid_max: f64,
        slider_min: f64,
        slider_max: f64,
        #[serde(default)]
        precision: u8,
        /// Integer slider (AE `PF_Param_SLIDER`).
        #[serde(default)]
        integer: bool,
        /// `none`, `percent`, `pixels`, `degrees` (display only).
        #[serde(default = "none_str")]
        display: String,
    },
    Angle,
    Checkbox,
    Color,
    Point,
    Point3d,
    /// Popup menu; the value is the 1-based index, as in AE.
    Popup {
        options: Vec<String>,
    },
    /// A layer of the same comp (value: prim_ref, none = this layer / none).
    Layer,
    /// Start of a twirly group; parameters until the matching `GroupEnd` belong to it.
    GroupStart,
    GroupEnd,
    /// Curves as a flat list of (channel, x, y) control points.
    CurvePoints,
    /// Opaque plugin-defined data (value: bytes).
    ArbitraryData,
    /// A mask of the layer (value: prim_ref to a Mask prim).
    Path,
    /// A button (no value; clicking triggers the plugin).
    Button,
    /// A read-only visualization derived from other parameters. Stores no value.
    Display,
}

fn none_str() -> String {
    "none".to_owned()
}

impl ParamKind {
    /// Value type stored in the `param:NNNN` attribute, if this control owns one.
    pub fn value_type(&self) -> Option<ValueType> {
        Some(match self {
            ParamKind::Slider { integer: true, .. } => ValueType::Int,
            ParamKind::Slider { .. } | ParamKind::Angle => ValueType::Float,
            ParamKind::Checkbox => ValueType::Bool,
            ParamKind::Color => ValueType::Color,
            ParamKind::Point => ValueType::Vec2,
            ParamKind::Point3d => ValueType::Vec3,
            ParamKind::Popup { .. } => ValueType::Int,
            ParamKind::Layer | ParamKind::Path => ValueType::PrimRef,
            ParamKind::CurvePoints => ValueType::FloatList,
            ParamKind::ArbitraryData => ValueType::Bytes,
            ParamKind::GroupStart
            | ParamKind::GroupEnd
            | ParamKind::Button
            | ParamKind::Display => return None,
        })
    }
}

/// Parameter flags (AE `PF_ParamFlag_*` / `PF_PUI_*` subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ParamFlags {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// The plugin wants to be told when the value changes (AE "supervise").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supervise: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cannot_animate: bool,
    /// The value never depends on time even if keyframed (AE `CANNOT_TIME_VARY`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cannot_time_vary: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// Parameter has a custom-drawn UI (control panel or comp viewer).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom_ui: bool,
    /// The default (a point) is a fraction of the layer size, as AE's
    /// `PF_ADD_POINT` percentages: an instance starts at `default × layer
    /// size`. A 3D point's z stays absolute.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relative_default: bool,
    /// The default is in AE reference-layer pixels (640 × 360), rather than layer pixels.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reference_point_default: bool,
}

/// Coordinate units of an unauthored effect point. Authored points use layer pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointDefaultSpace {
    Pixels,
    LayerFraction,
    AeReference,
}

impl ParamFlags {
    pub fn point_default_space(self) -> PointDefaultSpace {
        if self.relative_default {
            PointDefaultSpace::LayerFraction
        } else if self.reference_point_default {
            PointDefaultSpace::AeReference
        } else {
            PointDefaultSpace::Pixels
        }
    }
}

impl PointDefaultSpace {
    /// Resolve the default in layer pixels, relative to the layer's origin.
    pub fn layer_pixels(self, [x, y]: [f64; 2], [w, h]: [f64; 2]) -> [f64; 2] {
        match self {
            Self::Pixels => [x, y],
            Self::LayerFraction => [x * w, y * h],
            Self::AeReference => [x * w * (1. / 640.), y * h * (1. / 360.)],
        }
    }
}

/// One parameter of an effect definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EffectParamDef {
    /// Stable numeric ID (AE parameter index / disk ID).
    pub id: u32,
    pub name: String,
    /// AE parameter matchName if it has one (e.g. `ADBE Gaussian Blur 2-0001`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_name: Option<String>,
    #[serde(flatten)]
    pub kind: ParamKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "is_default_flags")]
    pub flags: ParamFlags,
}

fn is_default_flags(f: &ParamFlags) -> bool {
    *f == ParamFlags::default()
}

/// The plugin an effect comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PluginRef {
    pub id: String,
    pub version: String,
}

/// An effect type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EffectDef {
    /// Identity, e.g. `ADBE Gaussian Blur 2` or `com.example.glow`.
    pub match_name: String,
    pub name: String,
    #[serde(default)]
    pub category: String,
    /// `None` for built-in effects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginRef>,
    pub params: Vec<EffectParamDef>,
    /// Safe to render on several threads at once (AE multi-frame rendering).
    #[serde(default)]
    pub thread_safe: bool,
    #[serde(default)]
    pub gpu: bool,
}

impl EffectDef {
    pub fn param(&self, id: u32) -> Option<&EffectParamDef> {
        self.params.iter().find(|p| p.id == id)
    }
}

/// What the app knows about an `Effect` prim.
#[derive(Clone, Debug, PartialEq)]
pub enum EffectStatus<'a> {
    Known(&'a EffectDef),
    /// Provided by a plugin that is not installed: shown as a placeholder, data kept.
    MissingPlugin {
        match_name: String,
        plugin: Option<PluginRef>,
    },
}

/// Effect definitions by match name (built-in effects and plugins register here).
#[derive(Debug, Default)]
pub struct EffectRegistry {
    defs: IndexMap<String, EffectDef>,
}

impl EffectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a definition.
    pub fn register(&mut self, def: EffectDef) {
        self.defs.insert(def.match_name.clone(), def);
    }

    pub fn get(&self, match_name: &str) -> Option<&EffectDef> {
        self.defs.get(match_name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &EffectDef> {
        self.defs.values()
    }

    /// Remove every effect of a plugin (plugin unloaded).
    pub fn unregister_plugin(&mut self, plugin_id: &str) -> usize {
        let before = self.defs.len();
        self.defs
            .retain(|_, d| d.plugin.as_ref().is_none_or(|p| p.id != plugin_id));
        before - self.defs.len()
    }

    /// Status of an `Effect` prim; `None` if the prim is not an effect.
    pub fn status<'a>(&'a self, prim: &PrimView<'_>) -> Option<EffectStatus<'a>> {
        if !prim.is_a(&PrimType::EFFECT) {
            return None;
        }
        let match_name = prim.value("match_name")?.as_str()?.to_owned();
        Some(match self.defs.get(&match_name) {
            Some(def) => EffectStatus::Known(def),
            None => {
                let s = |n| prim.value(n).and_then(Value::as_str).map(str::to_owned);
                EffectStatus::MissingPlugin {
                    match_name,
                    plugin: s("plugin:id").map(|id| PluginRef {
                        id,
                        version: s("plugin:version").unwrap_or_default(),
                    }),
                }
            }
        })
    }

    /// Check a parameter value against a known effect definition. Unknown effects and
    /// parameters are accepted (placeholders keep whatever they hold).
    pub fn check_param(&self, match_name: &str, attr: &str, value: &Value) -> Result<(), String> {
        let (Some(def), Some(id)) = (self.get(match_name), param_id(attr)) else {
            return Ok(());
        };
        let Some(p) = def.param(id) else {
            return Ok(());
        };
        let Some(want) = p.kind.value_type() else {
            return Err(format!("parameter {id} ({}) has no value", p.name));
        };
        let got = value.value_type();
        if got != want && !(want == ValueType::Float && got == ValueType::Int) {
            return Err(format!(
                "parameter {id} ({}) expects {want:?}, got {got:?}",
                p.name
            ));
        }
        if let (ParamKind::Popup { options }, Value::Int(i)) = (&p.kind, value)
            && (*i < 1 || *i as usize > options.len())
        {
            return Err(format!(
                "parameter {id} ({}): popup index {i} out of 1..={}",
                p.name,
                options.len()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_names() {
        assert_eq!(param_attr(1), "param:0001");
        assert_eq!(param_id("param:0012"), Some(12));
        assert_eq!(param_id("opacity"), None);
        crate::error::validate_attr_name(&param_attr(9999)).unwrap();
    }

    #[test]
    fn definition_json_and_checks() {
        let def: EffectDef = serde_json::from_value(serde_json::json!({
            "match_name": "ADBE Gaussian Blur 2",
            "name": "Gaussian Blur",
            "category": "Blur & Sharpen",
            "params": [
                {"id": 1, "name": "Blurriness", "kind": "slider", "valid_min": 0.0, "valid_max": 3000.0,
                 "slider_min": 0.0, "slider_max": 50.0, "default": {"type": "float", "value": 0.0}},
                {"id": 2, "name": "Blur Dimensions", "kind": "popup",
                 "options": ["Horizontal and Vertical", "Horizontal", "Vertical"],
                 "default": {"type": "int", "value": 1}},
                {"id": 3, "name": "Repeat Edge Pixels", "kind": "checkbox"}
            ]
        }))
        .unwrap();
        let mut reg = EffectRegistry::new();
        reg.register(def);
        let m = "ADBE Gaussian Blur 2";
        reg.check_param(m, "param:0001", &Value::Float(12.0))
            .unwrap();
        reg.check_param(m, "param:0001", &Value::Int(12)).unwrap();
        assert!(
            reg.check_param(m, "param:0001", &Value::Bool(true))
                .is_err()
        );
        assert!(reg.check_param(m, "param:0002", &Value::Int(4)).is_err());
        reg.check_param(m, "param:0002", &Value::Int(3)).unwrap();
        reg.check_param(
            "com.missing.fx",
            "param:0001",
            &Value::String("kept".into()),
        )
        .unwrap();
        let json = serde_json::to_value(reg.get(m).unwrap()).unwrap();
        assert_eq!(json["params"][1]["kind"], "popup");
        assert_eq!(reg.unregister_plugin("none"), 0);
    }
}
