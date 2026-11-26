//! Persisted source-channel selections, shared with the Effect Controls panel.
use bae_model::Value;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Default, Serialize, Deserialize)]
/// A Cryptomatte pass and a stable object ID.
pub struct CryptoSelection {
    /// Qualified pass name. An empty name selects the first pass.
    pub layer: String,
    /// Original float bits of the chosen object.
    pub object: Option<u32>,
}
#[derive(Default, Serialize, Deserialize)]
/// EXtractoR channel assignment.
pub struct Routing {
    /// Source channel for each RGBA component. Empty names copy the input.
    pub channels: [String; 4],
}
#[derive(Default, Serialize, Deserialize)]
/// IDentifier input selection.
pub struct IdChannel {
    /// Qualified channel name; empty selects the object ID pass.
    pub name: String,
}

/// Decode a custom effect parameter, using defaults for an unset value.
pub fn read<T: Default + DeserializeOwned>(value: Option<&Value>) -> T {
    if let Some(Value::Bytes(data)) = value {
        serde_json::from_slice(data).unwrap_or_default()
    } else {
        T::default()
    }
}
/// Encode the selection for an undoable project edit.
pub fn value<T: Serialize>(settings: &T) -> Value {
    Value::Bytes(serde_json::to_vec(settings).expect("serializable channel selection"))
}
