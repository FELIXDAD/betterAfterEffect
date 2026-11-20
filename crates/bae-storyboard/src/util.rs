//! Write helpers used by the storyboard commands. Every write is a `bae-model` op in
//! the running transaction, so a command is exactly one undo step.

use std::collections::BTreeMap;

use bae_model::commands::{DeletePrim, DeletePrimParams, MovePrim, MovePrimParams};
use bae_model::schema;
use bae_model::{
    AttrSpec, Command, CommandError, ModelError, Op, PrimPath, PrimSpec, PrimTree, PrimType,
    Specifier, Stage, Transaction, Value,
};

pub(crate) fn fail(msg: impl Into<String>) -> CommandError {
    CommandError::Failed(msg.into())
}

pub(crate) fn child(parent: &PrimPath, name: &str) -> Result<PrimPath, CommandError> {
    parent
        .child(name)
        .map_err(|e| CommandError::from(ModelError::from(e)))
}

pub(crate) fn path(s: &str) -> PrimPath {
    PrimPath::new(s).expect("valid constant path")
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

/// Delete a prim and its subtree from every sublayer (built-in `prim.delete`).
pub(crate) fn delete(tx: &mut Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    DeletePrim.run(tx, DeletePrimParams { path: path.clone() })
}

/// Move a prim (and its subtree) in every sublayer (built-in `prim.move`).
pub(crate) fn move_to(
    tx: &mut Transaction<'_>,
    from: &PrimPath,
    to: &PrimPath,
) -> Result<(), CommandError> {
    MovePrim
        .run(
            tx,
            MovePrimParams {
                from: from.clone(),
                to: to.clone(),
                index: None,
            },
        )
        .map(|_| ())
}

/// Reorder the children of `folder` so the ones listed in `order` come in that order
/// (unlisted children go last), in every sublayer that has opinions about `folder`.
pub(crate) fn apply_order(
    tx: &mut Transaction<'_>,
    folder: &PrimPath,
    order: &[PrimPath],
) -> Result<(), CommandError> {
    let stack: Vec<_> = tx.stage().layer_stack().to_vec();
    for id in stack {
        let Some(layer) = tx.stage().sublayer(&id) else {
            continue;
        };
        if !layer.has_spec(folder) {
            continue;
        }
        let mut desired: Vec<(usize, usize, PrimPath)> = layer
            .children(folder)
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let rank = order.iter().position(|o| o == &p).unwrap_or(usize::MAX);
                (rank, i, p)
            })
            .collect();
        desired.sort();
        for (target, (_, _, p)) in desired.into_iter().enumerate() {
            let current = tx.stage().sublayer(&id).and_then(|s| s.child_index(&p));
            if current != Some(target) {
                tx.apply_to(
                    &id,
                    Op::MovePrim {
                        from: p.clone(),
                        to: p,
                        index: Some(target),
                    },
                )?;
            }
        }
    }
    Ok(())
}

/// The composed subtree at `path` as plain `def` specs (winning opinion of every
/// attribute), used to copy artwork.
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

/// Rewrite every `PrimRef`/`AssetRef` value inside `tree` whose target lies under
/// `from` so it points under `to` instead (parenting and mattes inside copied artwork).
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

/// Copy the composed subtree at `from` to the new path `to` (appended, or at `index`).
pub(crate) fn copy_tree(
    tx: &mut Transaction<'_>,
    from: &PrimPath,
    to: &PrimPath,
    index: Option<usize>,
) -> Result<(), CommandError> {
    let mut tree = composed_tree(tx.stage(), from);
    retarget_refs(&mut tree, from, to);
    let parent = to.parent().ok_or(ModelError::RootNotEditable)?;
    tx.ensure_spec(&parent)?;
    tx.apply(Op::RestorePrim {
        path: to.clone(),
        tree,
        index,
    })?;
    Ok(())
}
