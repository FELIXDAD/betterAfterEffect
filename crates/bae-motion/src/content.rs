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

// ---------------------------------------------------------------------------

/// `prims` in their parent's order; they must all be in one shape container
/// (a shape layer's or group's contents).
fn siblings(stage: &Stage, prims: &[PrimPath]) -> Result<(PrimPath, Vec<PrimPath>), CommandError> {
    let parent = prims
        .first()
        .and_then(PrimPath::parent)
        .ok_or_else(|| fail("give at least one shape"))?;
    if parent.name() != query::CONTENTS
        || prims.iter().any(|p| p.parent().as_ref() != Some(&parent))
    {
        return Err(fail("the shapes must be in the same group or layer"));
    }
    let ordered = stage
        .children(&parent)
        .into_iter()
        .filter(|c| prims.contains(c))
        .collect();
    Ok((parent, ordered))
}

/// Move `trees` (name, tree) into `parent` from `index` on, keeping their names.
fn move_in(
    tx: &mut Transaction<'_>,
    parent: &PrimPath,
    trees: Vec<(String, PrimTree)>,
    index: Option<usize>,
) -> Result<Vec<PrimPath>, CommandError> {
    let mut out = Vec::new();
    tx.ensure_spec(parent)?;
    for (k, (name, tree)) in trees.into_iter().enumerate() {
        let path = child(parent, &query::unique_child_name(tx.stage(), parent, &name))?;
        tx.apply(Op::RestorePrim {
            path: path.clone(),
            tree,
            index: index.map(|i| i + k),
        })?;
        out.push(path);
    }
    Ok(out)
}

/// Points outlining the shape item `item` at `t` in its container's space:
/// path vertices, through nested groups' transforms (skew left out).
fn outline_points(stage: &Stage, item: &PrimPath, t: Time, out: &mut Vec<[f64; 2]>) {
    let Some(ty) = stage.prim(item).and_then(|v| v.prim_type().cloned()) else {
        return;
    };
    if ty == PrimType::PATH {
        if let Some(Value::Bezier(b)) = query::sample_attr(stage, item, "path", t) {
            out.extend(b.vertices);
        }
    } else if ty == PrimType::SHAPE_GROUP {
        let Ok(inner) = child(item, query::CONTENTS) else {
            return;
        };
        let mut pts = Vec::new();
        for c in stage.children(&inner) {
            outline_points(stage, &c, t, &mut pts);
        }
        let v2 = |a: &str, d: [f64; 2]| match query::sample_attr(stage, item, a, t) {
            Some(Value::Vec2(v)) => v,
            _ => d,
        };
        let anchor = v2("transform:anchor_point", [0.0, 0.0]);
        let position = v2("transform:position", [0.0, 0.0]);
        let scale = v2("transform:scale", [100.0, 100.0]);
        let r = query::sample_attr(stage, item, "transform:rotation", t)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .to_radians();
        let (sin, cos) = r.sin_cos();
        out.extend(pts.into_iter().map(|[x, y]| {
            let (x, y) = (
                (x - anchor[0]) * scale[0] / 100.0,
                (y - anchor[1]) * scale[1] / 100.0,
            );
            [
                position[0] + x * cos - y * sin,
                position[1] + x * sin + y * cos,
            ]
        }));
    } else if let Some(b) = crate::shape::parametric_outline(stage, item, t) {
        out.extend(b.vertices);
    }
}

pub struct GroupShapes;

#[derive(Deserialize, JsonSchema)]
pub struct GroupShapesParams {
    /// Shape groups and items of one shape layer's or group's contents.
    pub items: Vec<PrimPath>,
    /// Layer time of the outline that places the group's anchor point.
    #[serde(default)]
    pub time: Option<Time>,
}

impl Command for GroupShapes {
    type Params = GroupShapesParams;
    type Output = ContentOutput;
    const ID: &'static str = "shape.group";
    const TITLE: &'static str = "Group Shapes";
    const DESCRIPTION: &'static str = "Layer > Group Shapes (Ctrl+G): put shape groups and items of one container into a new group (\"Group 1\") where the frontmost of them was, in their order.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ContentOutput, CommandError> {
        ensure_unlocked(tx.stage(), &p.items)?;
        let (parent, items) = siblings(tx.stage(), &p.items)?;
        let index = tx
            .stage()
            .children(&parent)
            .iter()
            .position(|c| items.contains(c));
        let trees: Vec<(String, PrimTree)> = items
            .iter()
            .map(|i| (i.name().to_owned(), composed_tree(tx.stage(), i)))
            .collect();
        // AE puts the new group's anchor point at the center of its contents'
        // bounding box (and its position there: nothing moves).
        let mut pts = Vec::new();
        for i in &items {
            outline_points(tx.stage(), i, p.time.unwrap_or(Time::ZERO), &mut pts);
        }
        let center = pts.split_first().map(|(first, rest)| {
            let (lo, hi) = rest.iter().fold((*first, *first), |(lo, hi), q| {
                (
                    [lo[0].min(q[0]), lo[1].min(q[1])],
                    [hi[0].max(q[0]), hi[1].max(q[1])],
                )
            });
            [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
        });
        crate::layer::delete_prims(tx, &items, Time::ZERO)?;
        let name = query::next_numbered_name(tx.stage(), &parent, crate::names::localized("Group"));
        let group = child(
            &parent,
            &query::unique_child_name(tx.stage(), &parent, &name),
        )?;
        create(tx, &group, PrimType::SHAPE_GROUP, index)?;
        set(tx, &group, "name", Value::String(name))?;
        if let Some(c) = center {
            set(tx, &group, "transform:anchor_point", Value::Vec2(c))?;
            set(tx, &group, "transform:position", Value::Vec2(c))?;
        }
        let inner = child(&group, query::CONTENTS)?;
        create(tx, &inner, PrimType::FOLDER, None)?;
        move_in(tx, &inner, trees, None)?;
        Ok(ContentOutput { prims: vec![group] })
    }
}

pub struct UngroupShapes;

#[derive(Deserialize, JsonSchema)]
pub struct UngroupShapesParams {
    /// Shape groups.
    pub groups: Vec<PrimPath>,
}

impl Command for UngroupShapes {
    type Params = UngroupShapesParams;
    type Output = ContentOutput;
    const ID: &'static str = "shape.ungroup";
    const TITLE: &'static str = "Ungroup Shapes";
    const DESCRIPTION: &'static str = "Layer > Ungroup Shapes (Ctrl+Shift+G): replace shape groups with their contents, in place.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ContentOutput, CommandError> {
        ensure_unlocked(tx.stage(), &p.groups)?;
        let mut out = Vec::new();
        for group in &p.groups {
            let stage = tx.stage();
            if !stage
                .prim(group)
                .is_some_and(|v| v.is_a(&PrimType::SHAPE_GROUP))
            {
                return Err(fail(format!("{group} is not a shape group")));
            }
            let parent = group
                .parent()
                .ok_or_else(|| fail("a group needs a parent"))?;
            let index = stage.children(&parent).iter().position(|c| c == group);
            let inner = child(group, query::CONTENTS)?;
            let trees: Vec<(String, PrimTree)> = stage
                .children(&inner)
                .iter()
                .map(|c| (c.name().to_owned(), composed_tree(stage, c)))
                .collect();
            crate::layer::delete_prims(tx, std::slice::from_ref(group), Time::ZERO)?;
            out.extend(move_in(tx, &parent, trees, index)?);
        }
        Ok(ContentOutput { prims: out })
    }
}
