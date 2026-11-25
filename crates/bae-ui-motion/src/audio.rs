//! Audio sources of composition layers.

use bae_model::{PrimPath, Stage, Value};
use bae_motion::query;
use std::path::PathBuf;

pub(crate) fn layer_file(stage: &Stage, layer: &PrimPath) -> Option<PathBuf> {
    let Value::PrimRef(asset) = query::static_value(&stage.prim(layer)?, "source")? else {
        return None;
    };
    let asset = stage.prim(&asset)?;
    if query::static_value(&asset, "has_audio")?.as_bool() != Some(true) {
        return None;
    }
    let path = query::static_value(&asset, "file:original")?;
    Some(PathBuf::from(path.as_str()?))
}
