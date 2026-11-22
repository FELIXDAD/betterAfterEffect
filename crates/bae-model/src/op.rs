//! Ops: the smallest serializable edits of one sublayer (ARCHITECTURE §4.5).
//!
//! Commands produce ops; every op applied to a sublayer yields its inverse, which is
//! what undo replays. Ops address prims by path *within one sublayer*, so the same
//! path can be edited independently in several sublayers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attr::{AttrSpec, Expression, Keyframe};
use crate::ids::SublayerId;
use crate::path::PrimPath;
use crate::prim::{PrimTree, PrimType, Reference, Specifier};
use crate::time::Time;
use crate::value::Value;

/// One edit of one sublayer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Create a prim spec. The parent must already have a spec in the sublayer
    /// (use `over` specs for ancestors that are defined in weaker sublayers).
    CreatePrim {
        path: PrimPath,
        #[serde(default)]
        specifier: Specifier,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prim_type: Option<PrimType>,
        /// Position among siblings; `None` appends.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Delete a prim spec and all its descendants.
    DeletePrim { path: PrimPath },
    /// Recreate a whole subtree (inverse of `DeletePrim`, paste). The tree's own
    /// `name` is ignored; the last segment of `path` is used.
    RestorePrim {
        path: PrimPath,
        tree: PrimTree,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Move and/or rename a prim spec (with its subtree). `index` reorders among the
    /// new siblings; moving to the same path with an index is a pure reorder.
    MovePrim {
        from: PrimPath,
        to: PrimPath,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    SetPrimType {
        path: PrimPath,
        prim_type: Option<PrimType>,
    },
    /// Turn an `over` into a `def` or back.
    SetSpecifier {
        path: PrimPath,
        specifier: Specifier,
    },
    /// Set (or with `None`, remove) the static value of an attribute.
    SetValue {
        path: PrimPath,
        attr: String,
        value: Option<Value>,
    },
    /// Insert or replace the keyframe at `time`.
    SetKeyframe {
        path: PrimPath,
        attr: String,
        time: Time,
        keyframe: Keyframe,
    },
    RemoveKeyframe {
        path: PrimPath,
        attr: String,
        time: Time,
    },
    SetExpression {
        path: PrimPath,
        attr: String,
        expression: Option<Expression>,
    },
    /// Replace this sublayer's whole opinion about an attribute (`None` clears it).
    SetAttr {
        path: PrimPath,
        attr: String,
        spec: Option<AttrSpec>,
    },
    SetReferences {
        path: PrimPath,
        references: Vec<Reference>,
    },
}

impl Op {
    /// The prim the op targets (for `MovePrim`, the source).
    pub fn path(&self) -> &PrimPath {
        match self {
            Op::CreatePrim { path, .. }
            | Op::DeletePrim { path }
            | Op::RestorePrim { path, .. }
            | Op::SetPrimType { path, .. }
            | Op::SetSpecifier { path, .. }
            | Op::SetValue { path, .. }
            | Op::SetKeyframe { path, .. }
            | Op::RemoveKeyframe { path, .. }
            | Op::SetExpression { path, .. }
            | Op::SetAttr { path, .. }
            | Op::SetReferences { path, .. } => path,
            Op::MovePrim { from, .. } => from,
        }
    }
}

/// An op addressed to a sublayer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Edit {
    pub sublayer: SublayerId,
    #[serde(flatten)]
    pub op: Op,
}

impl Edit {
    pub fn new(sublayer: SublayerId, op: Op) -> Self {
        Edit { sublayer, op }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape() {
        let e = Edit::new(
            SublayerId::root(),
            Op::SetValue {
                path: PrimPath::new("/a").unwrap(),
                attr: "opacity".into(),
                value: Some(Value::Float(50.0)),
            },
        );
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "sublayer": "root", "op": "set_value", "path": "/a", "attr": "opacity",
                "value": {"type": "float", "value": 50.0}
            })
        );
        assert_eq!(serde_json::from_value::<Edit>(json).unwrap(), e);
    }
}
