//! Attribute values.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::path::PrimPath;

/// A value stored in an attribute or keyframe.
///
/// Serialized as `{"type": "<kind>", "value": ...}` so that JSON exported for agents
/// and diffs is self-describing (an `Int` never turns into a `Float`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Float(f64),
    Int(i64),
    Bool(bool),
    String(String),
    Vec2([f64; 2]),
    Vec3([f64; 3]),
    /// Linear-light or display-referred RGBA, depending on the attribute; values may
    /// exceed 0..1 (32-bit float projects).
    Color([f64; 4]),
    /// A Bezier path (AE shape path / mask path, Ai path).
    Bezier(BezierPath),
    /// A token from a fixed set (blend mode, layer kind, popup menu entry …).
    Enum(String),
    /// Reference to an `Asset` prim.
    AssetRef(PrimPath),
    /// Reference to any prim (parent layer, track matte, layer parameter …).
    PrimRef(PrimPath),
    /// Opaque binary data (AE "arbitrary data" parameters, plugin state). Base64 in JSON.
    Bytes(
        #[serde(with = "base64_bytes")]
        #[schemars(with = "String")]
        Vec<u8>,
    ),
    /// A list of numbers (dash pattern, gradient stops encoded flat, …).
    FloatList(Vec<f64>),
}

/// The type of a [`Value`], used for schemas and validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Float,
    Int,
    Bool,
    String,
    Vec2,
    Vec3,
    Color,
    Bezier,
    Enum,
    AssetRef,
    PrimRef,
    Bytes,
    FloatList,
}

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Float(_) => ValueType::Float,
            Value::Int(_) => ValueType::Int,
            Value::Bool(_) => ValueType::Bool,
            Value::String(_) => ValueType::String,
            Value::Vec2(_) => ValueType::Vec2,
            Value::Vec3(_) => ValueType::Vec3,
            Value::Color(_) => ValueType::Color,
            Value::Bezier(_) => ValueType::Bezier,
            Value::Enum(_) => ValueType::Enum,
            Value::AssetRef(_) => ValueType::AssetRef,
            Value::PrimRef(_) => ValueType::PrimRef,
            Value::Bytes(_) => ValueType::Bytes,
            Value::FloatList(_) => ValueType::FloatList,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Float(v) => Some(v),
            Value::Int(v) => Some(v as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) | Value::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_prim_ref(&self) -> Option<&PrimPath> {
        match self {
            Value::PrimRef(p) | Value::AssetRef(p) => Some(p),
            _ => None,
        }
    }

    /// Number of animatable scalar dimensions (AE keyframe ease has one entry per dimension
    /// for non-spatial properties).
    pub fn dimensions(&self) -> usize {
        match self {
            Value::Float(_) | Value::Int(_) | Value::Bool(_) => 1,
            Value::Vec2(_) => 2,
            Value::Vec3(_) => 3,
            Value::Color(_) => 4,
            Value::FloatList(v) => v.len(),
            _ => 1,
        }
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_owned())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}
impl From<[f64; 2]> for Value {
    fn from(v: [f64; 2]) -> Self {
        Value::Vec2(v)
    }
}
impl From<[f64; 3]> for Value {
    fn from(v: [f64; 3]) -> Self {
        Value::Vec3(v)
    }
}

/// A cubic Bezier path in AE's vertex + tangent form. Tangents are relative to their
/// vertex, as in AE `Shape.inTangents` / `outTangents`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BezierPath {
    pub vertices: Vec<[f64; 2]>,
    pub in_tangents: Vec<[f64; 2]>,
    pub out_tangents: Vec<[f64; 2]>,
    pub closed: bool,
    /// Per-vertex feather (mask feather points); empty when unused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feather: Vec<[f64; 2]>,
}

impl BezierPath {
    /// Whether vertex and tangent arrays agree in length.
    pub fn is_consistent(&self) -> bool {
        self.vertices.len() == self.in_tangents.len()
            && self.vertices.len() == self.out_tangents.len()
    }
}

mod base64_bytes {
    //! Minimal standard base64 (RFC 4648, padded) so `Bytes` values stay readable in JSON
    //! without another dependency.
    use serde::{Deserialize, Deserializer, Serializer};

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, &'static str> {
        let s = s.trim_end_matches('=');
        let mut out = Vec::with_capacity(s.len() * 3 / 4);
        let mut acc = 0u32;
        let mut bits = 0;
        for c in s.bytes() {
            let v = ALPHABET
                .iter()
                .position(|&a| a == c)
                .ok_or("invalid base64")? as u32;
            acc = (acc << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        Ok(out)
    }

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = std::borrow::Cow::<str>::deserialize(d)?;
        decode(&s).map_err(serde::de::Error::custom)
    }

    #[cfg(test)]
    #[test]
    fn roundtrip() {
        for data in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            &[0, 255, 128, 7],
        ] {
            assert_eq!(decode(&encode(data)).unwrap(), data);
        }
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"fo"), "Zm8=");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_json() {
        let v = Value::Vec2([1.0, 2.5]);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"type":"vec2","value":[1.0,2.5]}"#);
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), v);
        // Int stays Int.
        let i: Value = serde_json::from_str(r#"{"type":"int","value":3}"#).unwrap();
        assert_eq!(i, Value::Int(3));
        let b = Value::Bytes(vec![1, 2, 3]);
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, r#"{"type":"bytes","value":"AQID"}"#);
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), b);
    }
}
