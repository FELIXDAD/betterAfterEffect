//! Text on a path (AE: Text > Path Options > Path): `text.set_path` makes one
//! of a text layer's masks the path its characters follow, or takes the text
//! off its path. The other Path Options are plain attributes (`attr.set_at`).

use bae_model::{Command, CommandError, Op, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::query;
use crate::util::{ensure_unlocked, fail, set};

/// The mask the text follows.
pub const PATH: &str = "text:path";

pub struct SetTextPath;

#[derive(Deserialize, JsonSchema)]
pub struct SetTextPathParams {
    /// The text layer.
    pub layer: PrimPath,
    /// One of the layer's masks; null = None (the text leaves the path).
    #[serde(default)]
    pub mask: Option<PrimPath>,
}

impl Command for SetTextPath {
    type Params = SetTextPathParams;
    type Output = ();
    const ID: &'static str = "text.set_path";
    const TITLE: &'static str = "Change Path";
    const DESCRIPTION: &'static str = "Lay a text layer's characters along one of its masks (AE: Text > Path Options > Path), or set Path to None.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let stage = tx.stage();
        if query::layer_kind(stage, &p.layer).as_deref() != Some("text") {
            return Err(fail(format!("{} is not a text layer", p.layer)));
        }
        ensure_unlocked(stage, [&p.layer])?;
        match p.mask {
            Some(mask) => {
                let own = mask
                    .parent()
                    .is_some_and(|m| p.layer.child("masks").ok() == Some(m))
                    && stage.prim(&mask).is_some_and(|v| v.is_a(&PrimType::MASK));
                if !own {
                    return Err(fail(format!("{mask} is not a mask of {}", p.layer)));
                }
                set(tx, &p.layer, PATH, Value::PrimRef(mask))
            }
            None => {
                tx.ensure_spec(&p.layer)?;
                tx.apply(Op::SetValue {
                    path: p.layer.clone(),
                    attr: PATH.into(),
                    value: None,
                })?;
                Ok(())
            }
        }
    }
}
