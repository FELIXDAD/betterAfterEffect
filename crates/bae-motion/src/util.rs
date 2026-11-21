//! Write helpers used by the motion commands. Every write is a `bae-model` op in the
//! running transaction, so a command is exactly one undo step.

use std::collections::BTreeMap;

use bae_model::schema;
use bae_model::{
    AttrSpec, CommandError, ModelError, Op, PrimPath, PrimSpec, PrimTree, PrimType, Specifier,
    Stage, Transaction, Value,
};

pub(crate) fn fail(msg: impl Into<String>) -> CommandError {
    CommandError::Failed(msg.into())
}

/// Commands do not edit locked layers (AE: a locked layer, its properties and
/// keyframes cannot be changed until it is unlocked; its Lock switch — a plain
/// `attr.set` — still can).
pub(crate) fn ensure_unlocked<'a>(
    stage: &Stage,
    paths: impl IntoIterator<Item = &'a PrimPath>,
) -> Result<(), CommandError> {
    for path in paths {
        if let Some(layer) = crate::query::locked_layer_of(stage, path) {
            let name = crate::query::display_name_of(stage, &layer);
            return Err(ModelError::LayerLocked { layer, name }.into());
        }
    }
    Ok(())
}

pub(crate) fn child(parent: &PrimPath, name: &str) -> Result<PrimPath, CommandError> {
    parent
        .child(name)
        .map_err(|e| CommandError::from(ModelError::from(e)))
}

/// Create a `def` prim of `ty` at `path` (its parent must exist on the stage).
pub(crate) fn create(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    ty: PrimType,
    index: Option<usize>,
) -> Result<(), CommandError> {
    let parent = path.parent().ok_or(ModelError::RootNotEditable)?;
    tx.stage().require_prim(&parent)?;
    if tx.stage().has_prim(path) {
        return Err(ModelError::PrimExists {
            sublayer: tx.target().clone(),
            path: path.clone(),
        }
        .into());
    }
    tx.ensure_spec(&parent)?;
    tx.apply(Op::CreatePrim {
        path: path.clone(),
        specifier: Specifier::Def,
        prim_type: Some(ty),
        index,
    })?;
    Ok(())
}

/// Make sure a folder exists at `path` (creating missing ancestors as folders).
pub(crate) fn ensure_folder(tx: &mut Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    if path.is_root() || tx.stage().has_prim(path) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        ensure_folder(tx, &parent)?;
    }
    create(tx, path, PrimType::FOLDER, None)
}

/// Set a static value after checking it against the prim schema.
pub(crate) fn set(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    value: impl Into<Value>,
) -> Result<(), CommandError> {
    let value = value.into();
    let ty = tx.stage().prim(path).and_then(|p| p.prim_type().cloned());
    schema::check_value(path, ty.as_ref(), attr, &value)?;
    tx.ensure_spec(path)?;
    tx.apply(Op::SetValue {
        path: path.clone(),
        attr: attr.to_owned(),
        value: Some(value),
    })?;
    Ok(())
}

/// Replace the edit target's whole opinion about an attribute.
pub(crate) fn write_attr(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    spec: AttrSpec,
) -> Result<(), CommandError> {
    tx.ensure_spec(path)?;
    tx.apply(Op::SetAttr {
        path: path.clone(),
        attr: attr.to_owned(),
        spec: (!spec.is_empty()).then_some(spec),
    })?;
    Ok(())
}

/// Require that keyframes are allowed on `attr` of `path`.
pub(crate) fn check_animatable(
    stage: &Stage,
    path: &PrimPath,
    attr: &str,
) -> Result<(), CommandError> {
    let ty = stage.prim(path).and_then(|p| p.prim_type().cloned());
    if schema::is_animatable(ty.as_ref(), attr) {
        Ok(())
    } else {
        Err(ModelError::NotAnimatable {
            path: path.clone(),
            attr: attr.to_owned(),
        }
        .into())
    }
}

/// The composed subtree at `path` as plain `def` specs (winning opinion of every
/// attribute), used to duplicate layers.
pub(crate) fn composed_tree(stage: &Stage, path: &PrimPath) -> PrimTree {
    let view = stage.prim(path);
    let mut attrs: BTreeMap<String, AttrSpec> = BTreeMap::new();
    let mut prim_type = None;
    if let Some(view) = &view {
        prim_type = view.prim_type().cloned();
        for name in view.attr_names() {
            if let Some(a) = view.attr(name) {
                attrs.insert(name.to_owned(), a.clone());
            }
        }
    }
    PrimTree {
        name: path.name().to_owned(),
        spec: PrimSpec {
            specifier: Specifier::Def,
            prim_type,
            attrs,
            references: view.map(|v| v.references().to_vec()).unwrap_or_default(),
        },
        children: stage
            .children(path)
            .iter()
            .map(|c| composed_tree(stage, c))
            .collect(),
    }
}

/// Rewrite every `PrimRef`/`AssetRef` value (static and keyframed) inside `tree`
/// whose target lies under `from` so it points under `to` instead.
pub(crate) fn retarget_refs(tree: &mut PrimTree, from: &PrimPath, to: &PrimPath) {
    fn fix(v: &mut Value, from: &PrimPath, to: &PrimPath) {
        if let Value::PrimRef(p) | Value::AssetRef(p) = v
            && let Some(n) = p.reparent(from, to)
        {
            *p = n;
        }
    }
    for a in tree.spec.attrs.values_mut() {
        if let Some(v) = &mut a.value {
            fix(v, from, to);
        }
        for k in a.keyframes.values_mut() {
            fix(&mut k.value, from, to);
        }
    }
    for c in &mut tree.children {
        retarget_refs(c, from, to);
    }
}
