//! Persistent selection of an unmerged image inside a Photoshop footage file.
use crate::{PrimView, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Deserialize, Serialize, JsonSchema)]
pub enum LayerKey {
    Id(u32),
    Index(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Deserialize, Serialize, JsonSchema)]
pub struct LayerImage {
    pub key: LayerKey,
    pub canvas: bool,
}

impl LayerImage {
    pub const KEY: &str = "psd:layer";
    pub const CANVAS: &str = "psd:canvas";

    pub fn from_asset(asset: &PrimView<'_>) -> Option<Self> {
        let Value::Int(id) = asset.value(Self::KEY)? else {
            return None;
        };
        let key = if *id >= 0 {
            LayerKey::Id(u32::try_from(*id).ok()?)
        } else {
            LayerKey::Index(u32::try_from(!*id).ok()?)
        };
        Some(Self {
            key,
            canvas: matches!(asset.value(Self::CANVAS), Some(Value::Bool(true))),
        })
    }

    pub fn attributes(self) -> [(&'static str, Value); 2] {
        let key = match self.key {
            LayerKey::Id(id) => i64::from(id),
            LayerKey::Index(index) => !i64::from(index),
        };
        [
            (Self::KEY, Value::Int(key)),
            (Self::CANVAS, Value::Bool(self.canvas)),
        ]
    }
}
