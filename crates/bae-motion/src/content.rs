//! Layer contents selected in the Timeline (shape groups and items, masks,
//! effects, text animators): Edit > Duplicate copies them next to the original
//! ("Rectangle 1" → "Rectangle 2"), Copy / Paste carries them to the same or
//! another layer, Delete removes them. Each is one command, one undo step.

use bae_model::{
    Command, CommandError, Op, PrimPath, PrimTree, PrimType, Stage, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query;
use crate::util::{child, composed_tree, create, ensure_unlocked, fail, set};

/// A copied layer content prim (for [`PasteContent`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CopiedContent {
    /// Where it was copied from.
    pub path: PrimPath,
    pub tree: PrimTree,
}

/// The clipboard form of `prims`.
pub fn copy_content(stage: &Stage, prims: &[PrimPath]) -> Vec<CopiedContent> {
    prims
        .iter()
        .filter(|p| stage.has_prim(p))
        .map(|p| CopiedContent {
            path: p.clone(),
            tree: composed_tree(stage, p),
        })
        .collect()
}

/// "Rectangle 2" → "Rectangle" (AE numbers copies from the base name).
fn base_name(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((base, n)) if !base.is_empty() && n.parse::<u32>().is_ok() => base,
        _ => name,
    }
}

/// AE's name for a copy among `parent`'s children: the same name when it is
/// free, else the next number of its base ("Rectangle 1" → "Rectangle 2";
/// an unnumbered "Gaussian Blur" counts as 1 → "Gaussian Blur 2").
fn copy_name(stage: &Stage, parent: &PrimPath, display: &str) -> String {
    let taken: Vec<String> = stage
        .children(parent)
        .iter()
        .map(|c| query::display_name_of(stage, c))
        .collect();
    if !taken.iter().any(|n| n == display) {
        return display.to_owned();
    }
    let base = base_name(display);
    let max = taken
        .iter()
        .filter_map(|n| match n.strip_prefix(base)? {
            "" => Some(1),
            rest => rest.strip_prefix(' ')?.parse::<u32>().ok(),
        })
        .max()
        .unwrap_or(1);
    format!("{base} {}", max + 1)
}

/// Where a copy goes in `parent` next to `at` (a selected sibling): shape
/// contents list front to back, so AE puts the copy above (in front of) `at`,
/// else on top; masks, effects and text animators get it below `at`, else at
/// the end.
fn slot(stage: &Stage, parent: &PrimPath, at: Option<&PrimPath>) -> Option<usize> {
    let shapes = parent.name() == query::CONTENTS;
    let i = at.and_then(|a| stage.children(parent).iter().position(|c| c == a));
    match (i, shapes) {
        (Some(i), true) => Some(i),
        (Some(i), false) => Some(i + 1),
        (None, true) => Some(0),
        (None, false) => None,
    }
}

/// Insert `tree` into `parent` at `index` with AE's next numbered name.
fn insert(
    tx: &mut Transaction<'_>,
    parent: &PrimPath,
    tree: PrimTree,
    display: &str,
    index: Option<usize>,
) -> Result<PrimPath, CommandError> {
    let name = copy_name(tx.stage(), parent, display);
    let path = child(
        parent,
        &query::unique_child_name(tx.stage(), parent, &query::sanitize_name(&name)),
    )?;
    tx.ensure_spec(parent)?;
    tx.apply(Op::RestorePrim {
        path: path.clone(),
        tree,
        index,
    })?;
    set(tx, &path, "name", Value::String(name))?;
    Ok(path)
}

#[derive(Serialize, JsonSchema)]
pub struct ContentOutput {
    /// The new prims, in order.
    pub prims: Vec<PrimPath>,
}

pub struct DuplicateContent;

#[derive(Deserialize, JsonSchema)]
pub struct DuplicateContentParams {
    pub prims: Vec<PrimPath>,
}

impl Command for DuplicateContent {
    type Params = DuplicateContentParams;
    type Output = ContentOutput;
    const ID: &'static str = "content.duplicate";
    const TITLE: &'static str = "Duplicate";
    const DESCRIPTION: &'static str = "Duplicate layer contents (shape groups and items, masks, effects, text animators) next to their originals, AE-numbered (Rectangle 1 → Rectangle 2).";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ContentOutput, CommandError> {
        ensure_unlocked(tx.stage(), &p.prims)?;
        let mut prims = Vec::new();
        for path in &p.prims {
            if query::is_layer(tx.stage(), path) {
                return Err(fail(format!("{path} is a layer (use layer.duplicate)")));
            }
            let parent = path.parent().ok_or_else(|| fail("no parent"))?;
            let index = slot(tx.stage(), &parent, Some(path));
            let display = query::display_name_of(tx.stage(), path);
            let tree = composed_tree(tx.stage(), path);
            prims.push(insert(tx, &parent, tree, &display, index)?);
        }
        Ok(ContentOutput { prims })
    }
}

pub struct PasteContent;

#[derive(Deserialize, JsonSchema)]
pub struct PasteContentParams {
    /// The container to paste into (a layer's contents, masks, effects …).
    pub parent: PrimPath,
    pub items: Vec<CopiedContent>,
    /// The selected child to paste next to: above it for shape contents,
    /// below it otherwise (else on top of shape contents, at the end of others).
    #[serde(default)]
    pub at: Option<PrimPath>,
}

impl Command for PasteContent {
    type Params = PasteContentParams;
    type Output = ContentOutput;
    const ID: &'static str = "content.paste";
    const TITLE: &'static str = "Paste";
    const DESCRIPTION: &'static str = "Paste copied layer contents into a container (a layer's contents, masks, effects or text animators), AE-numbered.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ContentOutput, CommandError> {
        ensure_unlocked(tx.stage(), [&p.parent])?;
        if !tx.stage().has_prim(&p.parent) {
            // A layer's first mask, effect or text animator.
            let folder = p.parent.name();
            let layer = p.parent.parent().filter(|l| query::is_layer(tx.stage(), l));
            let kind = layer.and_then(|l| query::layer_kind(tx.stage(), &l));
            let ok = match folder {
                "masks" | crate::effect::EFFECTS_FOLDER => kind.is_some(),
                crate::text_animators::ANIMATORS => kind.as_deref() == Some("text"),
                _ => false,
            };
            if !ok {
                return Err(fail(format!("cannot paste into {}", p.parent)));
            }
            create(tx, &p.parent, PrimType::FOLDER, None)?;
        }
        let mut index = slot(tx.stage(), &p.parent, p.at.as_ref());
        let mut prims = Vec::new();
        for item in p.items {
            let display = item
                .tree
                .spec
                .attrs
                .get("name")
                .and_then(|s| s.value.as_ref())
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| item.path.name().to_owned());
            prims.push(insert(tx, &p.parent, item.tree, &display, index)?);
            index = index.map(|i| i + 1);
        }
        Ok(ContentOutput { prims })
    }
}

pub struct DeleteContent;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteContentParams {
    pub prims: Vec<PrimPath>,
}

impl Command for DeleteContent {
    type Params = DeleteContentParams;
    type Output = ();
    const ID: &'static str = "content.delete";
    const TITLE: &'static str = "Delete";
    const DESCRIPTION: &'static str =
        "Delete layer contents (shape groups and items, masks, effects, text animators).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.prims)?;
        if p.prims.iter().any(|path| query::is_layer(tx.stage(), path)) {
            return Err(fail("use layer.delete for layers"));
        }
        crate::layer::delete_prims(tx, &p.prims, Time::ZERO)
    }
}
