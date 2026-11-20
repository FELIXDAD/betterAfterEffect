//! Attribute opinions: static value, keyframes and expression.
//!
//! This crate only *stores* animation data. Interpolation and expression evaluation
//! belong to `bae-anim` (workstream 03). The keyframe fields mirror AE's keyframe
//! model (`KeyframeInterpolationType`, `KeyframeEase`, spatial tangents, roving …) so
//! that AE projects round-trip without loss.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::time::Time;
use crate::value::Value;

/// One sublayer's opinion about one attribute.
///
/// An attribute is *animated* when it has keyframes; the static `value` is then only
/// the fallback used when keyframes are removed (AE keeps it the same way).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AttrSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Keyframes by time in flicks. JSON keys are decimal strings (`"0"`); both
    /// string and integer keys are accepted when reading.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "keyframes_de::deserialize"
    )]
    pub keyframes: BTreeMap<Time, Keyframe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<Expression>,
}

impl AttrSpec {
    pub fn with_value(value: impl Into<Value>) -> Self {
        AttrSpec {
            value: Some(value.into()),
            ..Default::default()
        }
    }

    /// No opinion at all.
    pub fn is_empty(&self) -> bool {
        self.value.is_none() && self.keyframes.is_empty() && self.expression.is_none()
    }

    pub fn is_animated(&self) -> bool {
        !self.keyframes.is_empty()
    }

    /// Time span whose evaluated value depends on the keyframe at `t`: from the previous
    /// keyframe to the next one (open-ended at the first/last keyframe).
    pub fn keyframe_influence(&self, t: Time) -> crate::time::TimeRange {
        let start = self
            .keyframes
            .range(..t)
            .next_back()
            .map_or(Time::MIN, |(k, _)| *k);
        let end = self
            .keyframes
            .range((std::ops::Bound::Excluded(t), std::ops::Bound::Unbounded))
            .next()
            .map_or(Time::MAX, |(k, _)| k.saturating_add(Time(1)));
        crate::time::TimeRange::new(start, end)
    }
}

mod keyframes_de {
    //! JSON object keys are strings; `#[serde(flatten)]` (used by `PrimTree`) buffers
    //! them, after which serde no longer converts `"0"` to an integer. Parse the time
    //! key ourselves so keyframes read back in every context.
    use std::collections::BTreeMap;
    use std::fmt;

    use serde::Deserializer;
    use serde::de::{self, MapAccess, Visitor};

    use super::Keyframe;
    use crate::time::Time;

    struct TimeKey(i64);

    impl<'de> de::Deserialize<'de> for TimeKey {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl Visitor<'_> for V {
                type Value = TimeKey;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a time in flicks (integer or decimal string)")
                }
                fn visit_i64<E: de::Error>(self, v: i64) -> Result<TimeKey, E> {
                    Ok(TimeKey(v))
                }
                fn visit_u64<E: de::Error>(self, v: u64) -> Result<TimeKey, E> {
                    i64::try_from(v).map(TimeKey).map_err(E::custom)
                }
                fn visit_str<E: de::Error>(self, v: &str) -> Result<TimeKey, E> {
                    v.trim().parse().map(TimeKey).map_err(E::custom)
                }
            }
            d.deserialize_any(V)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<Time, Keyframe>, D::Error> {
        struct M;
        impl<'de> Visitor<'de> for M {
            type Value = BTreeMap<Time, Keyframe>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of time → keyframe")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((TimeKey(t), k)) = map.next_entry::<TimeKey, Keyframe>()? {
                    out.insert(Time(t), k);
                }
                Ok(out)
            }
        }
        d.deserialize_map(M)
    }
}

/// Keyframe interpolation on one side of a keyframe (AE `KeyframeInterpolationType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    #[default]
    Linear,
    Bezier,
    Hold,
}

/// Temporal ease of one dimension (AE `KeyframeEase`): speed in units per second and
/// influence in percent (0.1 – 100).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ease {
    pub speed: f64,
    pub influence: f64,
}

impl Ease {
    /// AE's default influence for new Bezier keyframes and Easy Ease.
    pub const DEFAULT_INFLUENCE: f64 = 100.0 / 3.0;
}

impl Default for Ease {
    fn default() -> Self {
        Ease {
            speed: 0.0,
            influence: Self::DEFAULT_INFLUENCE,
        }
    }
}

/// A keyframe. Its time is the key in [`AttrSpec::keyframes`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Keyframe {
    pub value: Value,
    #[serde(default)]
    pub interp_in: Interpolation,
    #[serde(default)]
    pub interp_out: Interpolation,
    /// Incoming temporal ease, one entry per dimension (one entry for spatial properties).
    /// Empty means "AE default".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ease_in: Vec<Ease>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ease_out: Vec<Ease>,
    /// Spatial tangents (position-like properties), relative to the keyframe value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_in: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_out: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "KeyframeFlags::is_empty")]
    pub flags: KeyframeFlags,
    /// AE keyframe label color index (0 = none).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub label: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl Keyframe {
    /// A linear keyframe with AE defaults.
    pub fn linear(value: impl Into<Value>) -> Self {
        Keyframe {
            value: value.into(),
            interp_in: Interpolation::Linear,
            interp_out: Interpolation::Linear,
            ease_in: Vec::new(),
            ease_out: Vec::new(),
            spatial_in: None,
            spatial_out: None,
            flags: KeyframeFlags::default(),
            label: 0,
        }
    }

    /// A hold keyframe.
    pub fn hold(value: impl Into<Value>) -> Self {
        Keyframe {
            interp_out: Interpolation::Hold,
            ..Self::linear(value)
        }
    }
}

/// Boolean keyframe options (AE: temporal/spatial continuous, auto-Bezier, roving).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct KeyframeFlags {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub temporal_continuous: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub temporal_auto_bezier: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spatial_continuous: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spatial_auto_bezier: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub roving: bool,
}

impl KeyframeFlags {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// An expression (AE expressions are JavaScript; evaluated by `bae-anim`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Expression {
    pub source: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Expression {
    pub fn new(source: impl Into<String>) -> Self {
        Expression {
            source: source.into(),
            enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TimeRange;

    #[test]
    fn influence_range() {
        let mut a = AttrSpec::default();
        for t in [10, 20, 30] {
            a.keyframes.insert(Time(t), Keyframe::linear(t as f64));
        }
        assert_eq!(
            a.keyframe_influence(Time(20)),
            TimeRange::new(Time(10), Time(31))
        );
        assert_eq!(
            a.keyframe_influence(Time(10)),
            TimeRange::new(Time::MIN, Time(21))
        );
        assert_eq!(
            a.keyframe_influence(Time(30)),
            TimeRange::new(Time(20), Time::MAX)
        );
    }

    #[test]
    fn keyframes_read_back_through_flatten() {
        use crate::prim::{PrimSpec, PrimTree, PrimType};
        let mut a = AttrSpec::default();
        a.keyframes.insert(Time(0), Keyframe::linear(1.0));
        a.keyframes.insert(Time(-5), Keyframe::hold(2.0));
        let mut spec = PrimSpec::def(PrimType::LAYER);
        spec.attrs.insert("opacity".into(), a);
        let tree = PrimTree {
            name: "title".into(),
            spec,
            children: Vec::new(),
        };
        let json = serde_json::to_string(&tree).unwrap();
        assert_eq!(serde_json::from_str::<PrimTree>(&json).unwrap(), tree);
        // Integer keys (hand-written JSON) are accepted too.
        let k: AttrSpec = serde_json::from_str(
            r#"{"keyframes": {"7": {"value": {"type": "float", "value": 1.0}}}}"#,
        )
        .unwrap();
        assert!(k.keyframes.contains_key(&Time(7)));
    }

    #[test]
    fn compact_json() {
        let k = Keyframe::linear(1.0);
        assert_eq!(
            serde_json::to_string(&k).unwrap(),
            r#"{"value":{"type":"float","value":1.0},"interp_in":"linear","interp_out":"linear"}"#
        );
        let mut a = AttrSpec::default();
        a.keyframes.insert(Time(5), Keyframe::hold(2.0));
        let back: AttrSpec = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
    }
}
