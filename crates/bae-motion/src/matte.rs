//! `layer.set_track_matte`: AE 2023+ track mattes — any layer of the comp as the
//! matte, Alpha or Luma, inverted or not.

use bae_model::{Command, CommandError, PrimPath, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::query;
use crate::util::{ensure_unlocked, fail, set};

/// Track matte types, as stored in `track_matte`.
pub const MATTES: [&str; 5] = ["none", "alpha", "alpha_inverted", "luma", "luma_inverted"];

pub struct SetTrackMatte;

#[derive(Deserialize, JsonSchema)]
pub struct SetTrackMatteParams {
    pub layers: Vec<PrimPath>,
    /// none, alpha, alpha_inverted, luma or luma_inverted.
    pub matte: String,
    /// The matte layer (another layer of the same comp). Omitted: unchanged (a
    /// layer without one uses the layer above). Its Video switch is turned off, as
    /// AE does when a layer becomes a matte.
    #[serde(default)]
    pub source: Option<PrimPath>,
}

impl Command for SetTrackMatte {
    type Params = SetTrackMatteParams;
    type Output = ();
    const ID: &'static str = "layer.set_track_matte";
    const TITLE: &'static str = "Set Track Matte";
    const DESCRIPTION: &'static str = "Set layers' track matte: the type (none, alpha, alpha_inverted, luma, luma_inverted) and optionally the matte layer (Timeline Track Matte column, Layer > Track Matte).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        if !MATTES.contains(&p.matte.as_str()) {
            return Err(fail(format!("unknown track matte `{}`", p.matte)));
        }
        ensure_unlocked(tx.stage(), &p.layers)?;
        if let Some(s) = &p.source {
            if p.layers.contains(s) {
                return Err(fail("a layer cannot be its own track matte"));
            }
            let comp = |l: &PrimPath| query::owning_comp(tx.stage(), l);
            if p.layers.iter().any(|l| comp(l) != comp(s)) || !query::is_layer(tx.stage(), s) {
                return Err(fail(format!("{s} is not a layer of the same comp")));
            }
        }
        for l in &p.layers {
            set(tx, l, "track_matte", Value::Enum(p.matte.clone()))?;
            if let Some(s) = &p.source {
                set(tx, l, "track_matte_layer", Value::PrimRef(s.clone()))?;
            }
        }
        if let Some(s) = &p.source {
            set(tx, s, "enabled", Value::Bool(false))?;
        }
        Ok(())
    }
}
