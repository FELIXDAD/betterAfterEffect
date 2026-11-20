//! Built-in model commands: create/delete/move/rename prims, attributes, keyframes,
//! expressions, references, sublayers and the edit target.
//!
//! Attribute edits write to the transaction's target sublayer, creating `over` specs
//! as needed. Namespace edits (delete, move, rename) apply to *every* sublayer in the
//! stack that has a spec at the path, so overrides follow their prim.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attr::{Expression, Keyframe};
use crate::command::{Command, CommandError, CommandRegistry};
use crate::error::ModelError;
use crate::ids::SublayerId;
use crate::op::Op;
use crate::path::PrimPath;
use crate::prim::{PrimType, Reference, Specifier};
use crate::stage::Transaction;
use crate::time::Time;
use crate::value::Value;

/// Register every built-in command.
pub fn register_builtins(r: &mut CommandRegistry) {
    let results = [
        r.register(CreatePrim),
        r.register(DeletePrim),
        r.register(MovePrim),
        r.register(RenamePrim),
        r.register(SetAttr),
        r.register(ClearAttr),
        r.register(SetKey),
        r.register(RemoveKey),
        r.register(SetExpression),
        r.register(SetReferences),
        r.register(CreateSublayer),
        r.register(SetEditTarget),
    ];
    for res in results {
        res.expect("built-in command IDs are unique");
    }
}

/// Output of commands that produce a prim path.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PathOutput {
    pub path: PrimPath,
}

fn sublayers_with_spec(tx: &Transaction<'_>, path: &PrimPath) -> Vec<SublayerId> {
    tx.stage()
        .layer_stack()
        .iter()
        .filter(|id| tx.stage().sublayer(id).is_some_and(|s| s.has_spec(path)))
        .cloned()
        .collect()
}

fn require_prim(tx: &Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    tx.stage().require_prim(path)?;
    if path.is_root() {
        return Err(ModelError::RootNotEditable.into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------

pub struct CreatePrim;

#[derive(Deserialize, JsonSchema)]
pub struct CreatePrimParams {
    /// Path of the new prim. Its parent must exist.
    pub path: PrimPath,
    /// Prim type, e.g. Comp, Layer, ShapeGroup, Folder, Asset.
    pub prim_type: PrimType,
    /// Position among siblings (0 = first); omitted appends.
    #[serde(default)]
    pub index: Option<usize>,
}

impl Command for CreatePrim {
    type Params = CreatePrimParams;
    type Output = PathOutput;
    const ID: &'static str = "prim.create";
    const TITLE: &'static str = "Create Prim";
    const DESCRIPTION: &'static str =
        "Create a prim (comp, layer, shape item, folder …) under an existing parent.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PathOutput, CommandError> {
        let parent = p.path.parent().ok_or(ModelError::RootNotEditable)?;
        tx.stage().require_prim(&parent)?;
        if tx.stage().has_prim(&p.path) {
            return Err(ModelError::PrimExists {
                sublayer: tx.target().clone(),
                path: p.path,
            }
            .into());
        }
        tx.ensure_spec(&parent)?;
        tx.apply(Op::CreatePrim {
            path: p.path.clone(),
            specifier: Specifier::Def,
            prim_type: Some(p.prim_type),
            index: p.index,
        })?;
        Ok(PathOutput { path: p.path })
    }
}

pub struct DeletePrim;

#[derive(Deserialize, JsonSchema)]
pub struct DeletePrimParams {
    /// Prim to delete, with all its descendants, from every sublayer that has it.
    pub path: PrimPath,
}

impl Command for DeletePrim {
    type Params = DeletePrimParams;
    type Output = ();
    const ID: &'static str = "prim.delete";
    const TITLE: &'static str = "Delete Prim";
    const DESCRIPTION: &'static str = "Delete a prim and its descendants.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        for id in sublayers_with_spec(tx, &p.path) {
            tx.apply_to(
                &id,
                Op::DeletePrim {
                    path: p.path.clone(),
                },
            )?;
        }
        Ok(())
    }
}

pub struct MovePrim;

#[derive(Deserialize, JsonSchema)]
pub struct MovePrimParams {
    pub from: PrimPath,
    /// New path. The new parent must exist; a different last segment renames.
    pub to: PrimPath,
    /// Position among the new siblings; omitted keeps the position (same parent) or appends.
    #[serde(default)]
    pub index: Option<usize>,
}

fn move_everywhere(
    tx: &mut Transaction<'_>,
    from: &PrimPath,
    to: &PrimPath,
    index: Option<usize>,
) -> Result<(), CommandError> {
    require_prim(tx, from)?;
    let new_parent = to.parent().ok_or(ModelError::RootNotEditable)?;
    tx.stage().require_prim(&new_parent)?;
    if from != to && tx.stage().has_prim(to) {
        return Err(ModelError::PrimExists {
            sublayer: tx.target().clone(),
            path: to.clone(),
        }
        .into());
    }
    for id in sublayers_with_spec(tx, from) {
        tx.ensure_spec_in(&id, &new_parent)?;
        tx.apply_to(
            &id,
            Op::MovePrim {
                from: from.clone(),
                to: to.clone(),
                index,
            },
        )?;
    }
    Ok(())
}

impl Command for MovePrim {
    type Params = MovePrimParams;
    type Output = PathOutput;
    const ID: &'static str = "prim.move";
    const TITLE: &'static str = "Move Prim";
    const DESCRIPTION: &'static str =
        "Move, reorder or rename a prim (with its subtree) in every sublayer.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PathOutput, CommandError> {
        move_everywhere(tx, &p.from, &p.to, p.index)?;
        Ok(PathOutput { path: p.to })
    }
}

pub struct RenamePrim;

#[derive(Deserialize, JsonSchema)]
pub struct RenamePrimParams {
    pub path: PrimPath,
    /// New prim name (letters, digits, '_' or '-'). Display names are attributes.
    pub name: String,
}

impl Command for RenamePrim {
    type Params = RenamePrimParams;
    type Output = PathOutput;
    const ID: &'static str = "prim.rename";
    const TITLE: &'static str = "Rename Prim";
    const DESCRIPTION: &'static str = "Change a prim's name (the last path segment).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PathOutput, CommandError> {
        let parent = p.path.parent().ok_or(ModelError::RootNotEditable)?;
        let to = parent.child(&p.name).map_err(ModelError::from)?;
        move_everywhere(tx, &p.path, &to, None)?;
        Ok(PathOutput { path: to })
    }
}

pub struct SetAttr;

#[derive(Deserialize, JsonSchema)]
pub struct SetAttrParams {
    pub path: PrimPath,
    /// Attribute name, e.g. opacity, transform:position.
    pub attr: String,
    /// New static value. On an animated attribute use key.set instead.
    pub value: Value,
}

impl Command for SetAttr {
    type Params = SetAttrParams;
    type Output = ();
    const ID: &'static str = "attr.set";
    const TITLE: &'static str = "Set Value";
    const DESCRIPTION: &'static str =
        "Set the static value of an attribute in the edit target sublayer.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        tx.ensure_spec(&p.path)?;
        tx.apply(Op::SetValue {
            path: p.path,
            attr: p.attr,
            value: Some(p.value),
        })?;
        Ok(())
    }
}

pub struct ClearAttr;

#[derive(Deserialize, JsonSchema)]
pub struct ClearAttrParams {
    pub path: PrimPath,
    pub attr: String,
}

impl Command for ClearAttr {
    type Params = ClearAttrParams;
    type Output = ();
    const ID: &'static str = "attr.clear";
    const TITLE: &'static str = "Clear Attribute";
    const DESCRIPTION: &'static str = "Remove the edit target's opinion (value, keyframes, expression) about an attribute, revealing weaker sublayers.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        let has = tx
            .stage()
            .sublayer(tx.target())
            .and_then(|s| s.attr(&p.path, &p.attr))
            .is_some();
        if has {
            tx.apply(Op::SetAttr {
                path: p.path,
                attr: p.attr,
                spec: None,
            })?;
        }
        Ok(())
    }
}

pub struct SetKey;

#[derive(Deserialize, JsonSchema)]
pub struct SetKeyParams {
    pub path: PrimPath,
    pub attr: String,
    /// Keyframe time in flicks (705600000 per second).
    pub time: Time,
    pub keyframe: Keyframe,
}

impl Command for SetKey {
    type Params = SetKeyParams;
    type Output = ();
    const ID: &'static str = "key.set";
    const TITLE: &'static str = "Set Keyframe";
    const DESCRIPTION: &'static str = "Add or replace a keyframe in the edit target sublayer.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        tx.ensure_spec(&p.path)?;
        tx.apply(Op::SetKeyframe {
            path: p.path,
            attr: p.attr,
            time: p.time,
            keyframe: p.keyframe,
        })?;
        Ok(())
    }
}

pub struct RemoveKey;

#[derive(Deserialize, JsonSchema)]
pub struct RemoveKeyParams {
    pub path: PrimPath,
    pub attr: String,
    /// Keyframe time in flicks.
    pub time: Time,
}

impl Command for RemoveKey {
    type Params = RemoveKeyParams;
    type Output = ();
    const ID: &'static str = "key.remove";
    const TITLE: &'static str = "Remove Keyframe";
    const DESCRIPTION: &'static str = "Remove a keyframe from the edit target sublayer.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        tx.apply(Op::RemoveKeyframe {
            path: p.path,
            attr: p.attr,
            time: p.time,
        })?;
        Ok(())
    }
}

pub struct SetExpression;

#[derive(Deserialize, JsonSchema)]
pub struct SetExpressionParams {
    pub path: PrimPath,
    pub attr: String,
    /// JavaScript expression source (AE expression syntax); null removes the expression.
    pub source: Option<String>,
    /// Whether the expression is enabled (default true).
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Command for SetExpression {
    type Params = SetExpressionParams;
    type Output = ();
    const ID: &'static str = "attr.set_expression";
    const TITLE: &'static str = "Set Expression";
    const DESCRIPTION: &'static str = "Set, disable or remove an attribute's expression.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        tx.ensure_spec(&p.path)?;
        tx.apply(Op::SetExpression {
            path: p.path,
            attr: p.attr,
            expression: p.source.map(|source| Expression {
                source,
                enabled: p.enabled,
            }),
        })?;
        Ok(())
    }
}

pub struct SetReferences;

#[derive(Deserialize, JsonSchema)]
pub struct SetReferencesParams {
    pub path: PrimPath,
    /// Prims whose subtrees are composed under this prim (empty list clears).
    pub references: Vec<Reference>,
}

impl Command for SetReferences {
    type Params = SetReferencesParams;
    type Output = ();
    const ID: &'static str = "prim.set_references";
    const TITLE: &'static str = "Set References";
    const DESCRIPTION: &'static str =
        "Set the prims (possibly in other projects) that a prim references.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_prim(tx, &p.path)?;
        tx.ensure_spec(&p.path)?;
        tx.apply(Op::SetReferences {
            path: p.path,
            references: p.references,
        })?;
        Ok(())
    }
}

pub struct CreateSublayer;

#[derive(Deserialize, JsonSchema)]
pub struct CreateSublayerParams {
    /// New sublayer ID, e.g. anim/shot010 or agent/claude-01.
    pub id: SublayerId,
    /// Position in the layer stack, 0 = strongest (default).
    #[serde(default)]
    pub index: usize,
}

impl Command for CreateSublayer {
    type Params = CreateSublayerParams;
    type Output = ();
    const ID: &'static str = "sublayer.create";
    const TITLE: &'static str = "New Sublayer";
    const DESCRIPTION: &'static str = "Add an empty sublayer to the layer stack (not undoable).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        tx.create_sublayer(p.id, p.index)?;
        Ok(())
    }
}

pub struct SetEditTarget;

#[derive(Deserialize, JsonSchema)]
pub struct SetEditTargetParams {
    /// Sublayer that subsequent edits write to.
    pub id: SublayerId,
}

impl Command for SetEditTarget {
    type Params = SetEditTargetParams;
    type Output = ();
    const ID: &'static str = "sublayer.set_edit_target";
    const TITLE: &'static str = "Set Edit Target";
    const DESCRIPTION: &'static str =
        "Choose which sublayer edits are written to (session state, not undoable).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        tx.set_edit_target(&p.id)?;
        Ok(())
    }
}
