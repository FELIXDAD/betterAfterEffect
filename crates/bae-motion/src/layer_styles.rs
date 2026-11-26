use crate::{
    query,
    util::{child, composed_tree, create, ensure_unlocked, fail, set},
};
use bae_model::{
    Command, CommandError, Op, PrimPath, PrimType, Transaction, Value,
    effect::param_attr,
    layer_styles::{self as styles, Group},
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Edit the selected layers' Photoshop styles in one undo step.
pub struct EditStyles;

#[derive(JsonSchema, Deserialize)]
#[serde(rename_all = "snake_case", tag = "operation")]
pub enum Operation {
    Add { style: String },
    ShowAll,
    RemoveAll,
    Convert,
}

#[derive(JsonSchema, Deserialize)]
pub struct Params {
    pub layers: Vec<PrimPath>,
    #[serde(flatten)]
    pub operation: Operation,
}

fn copy_source(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    path: &PrimPath,
    source: &PrimPath,
) -> Result<(), CommandError> {
    let tree = composed_tree(tx.stage(), source);
    tx.apply(Op::RestorePrim {
        path: path.clone(),
        tree,
        index: None,
    })?;
    set(tx, layer, styles::USE_SOURCE, Value::Bool(false))?;
    for g in styles::GROUPS {
        let exists = tx
            .stage()
            .prim(path)
            .and_then(|v| v.attr(&param_attr(g.enabled)))
            .is_some();
        if exists || g.enabled == 0 {
            add_group(tx, path, g, true)?;
        }
    }
    Ok(())
}

fn add_group(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    group: &Group,
    enabled: bool,
) -> Result<(), CommandError> {
    let attr = param_attr(group.enabled);
    if tx.stage().prim(path).and_then(|v| v.attr(&attr)).is_none() {
        set(tx, path, &attr, Value::Bool(enabled))?;
    }
    for f in group.fields {
        let attr = param_attr(f.id);
        if tx.stage().prim(path).and_then(|v| v.attr(&attr)).is_none() {
            set(tx, path, &attr, f.initial())?;
        }
    }
    Ok(())
}

impl Command for EditStyles {
    type Params = Params;
    type Output = ();
    const ID: &'static str = "layer.styles";
    const TITLE: &'static str = "Layer Styles";
    const DESCRIPTION: &'static str = "Add, reveal, remove or make source layer styles editable.";
    fn run(&self, tx: &mut Transaction<'_>, p: Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        for layer in &p.layers {
            if !query::is_layer(tx.stage(), layer)
                || matches!(
                    query::layer_kind(tx.stage(), layer).as_deref(),
                    Some("camera" | "light" | "null")
                )
            {
                return Err(fail(format!("{layer} has no image")));
            }
            let path = child(layer, styles::PRIM)?;
            match &p.operation {
                Operation::RemoveAll => {
                    if styles::source_styles(tx.stage(), layer).is_some() {
                        set(tx, layer, styles::USE_SOURCE, Value::Bool(false))?;
                    }
                    crate::effect::delete_all(tx, std::slice::from_ref(&path))?;
                }
                Operation::Convert => {
                    if !tx.stage().has_prim(&path)
                        && let Some(source) = styles::source_styles(tx.stage(), layer)
                    {
                        copy_source(tx, layer, &path, &source)?;
                    }
                }
                op => {
                    if !tx.stage().has_prim(&path)
                        && let Some(source) = styles::effective_styles(tx.stage(), layer)
                    {
                        copy_source(tx, layer, &path, &source)?;
                    }
                    if !tx.stage().has_prim(&path) {
                        create(tx, &path, PrimType::EFFECT, None)?;
                        set(
                            tx,
                            &path,
                            "match_name",
                            Value::String(styles::RENDER_ID.into()),
                        )?;
                        set(tx, &path, "name", Value::String("Layer Styles".into()))?;
                    }
                    add_group(tx, &path, &styles::GROUPS[0], true)?;
                    if let Operation::Add { style } = op {
                        let g = styles::group(style)
                            .ok_or_else(|| fail(format!("unknown layer style {style}")))?;
                        let attr = param_attr(g.enabled);
                        if tx
                            .stage()
                            .prim(&path)
                            .and_then(|v| v.value(&attr))
                            .and_then(Value::as_bool)
                            == Some(false)
                        {
                            set(tx, &path, &attr, Value::Bool(true))?;
                        } else {
                            add_group(tx, &path, g, true)?;
                        }
                    } else {
                        for g in &styles::GROUPS[1..] {
                            add_group(tx, &path, g, false)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
