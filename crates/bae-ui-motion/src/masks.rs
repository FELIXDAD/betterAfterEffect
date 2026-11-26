//! Layer ▸ Mask: New Mask (Cmd/Ctrl+Shift+N), the Mask Shape / Feather /
//! Opacity / Expansion dialogs, Reset Mask, Remove Mask, Remove All Masks,
//! Mode, Inverted (Cmd/Ctrl+Shift+I), Locked, Unlock All Masks and Lock Other
//! Masks. They act on the selected masks (a mask property selected in the
//! Timeline), else on every mask of the selected layers, as AE does.

use bae_model::{BezierPath, PrimPath, PrimType, Stage, Time, Value};
use bae_motion::query;
use bae_ui::PanelContext;
use serde_json::json;

use crate::session::SharedSession;

pub(crate) mod ids {
    pub const NEW_MASK: &str = "motion.layer.new_mask";
    pub const MASK_SHAPE: &str = "motion.layer.mask_shape";
    pub const MASK_FEATHER: &str = "motion.layer.mask_feather";
    pub const MASK_OPACITY: &str = "motion.layer.mask_opacity";
    pub const MASK_EXPANSION: &str = "motion.layer.mask_expansion";
    pub const RESET_MASK: &str = "motion.layer.reset_mask";
    pub const REMOVE_MASK: &str = "motion.layer.remove_mask";
    pub const REMOVE_ALL_MASKS: &str = "motion.layer.remove_all_masks";
    /// Params `{"mode": "add" | "subtract" | …}`.
    pub const MASK_MODE: &str = "motion.layer.mask_mode";
    pub const MASK_INVERTED: &str = "motion.layer.mask_inverted";
    pub const MASK_LOCKED: &str = "motion.layer.mask_locked";
    pub const UNLOCK_ALL_MASKS: &str = "motion.layer.unlock_all_masks";
    pub const LOCK_OTHER_MASKS: &str = "motion.layer.lock_other_masks";
}

/// Whether `id` is one of this module's actions.
pub(crate) fn is_mask_action(id: &str) -> bool {
    [
        ids::NEW_MASK,
        ids::MASK_SHAPE,
        ids::MASK_FEATHER,
        ids::MASK_OPACITY,
        ids::MASK_EXPANSION,
        ids::RESET_MASK,
        ids::REMOVE_MASK,
        ids::REMOVE_ALL_MASKS,
        ids::MASK_MODE,
        ids::MASK_INVERTED,
        ids::MASK_LOCKED,
        ids::UNLOCK_ALL_MASKS,
        ids::LOCK_OTHER_MASKS,
    ]
    .contains(&id)
}

/// The masks of `layer`, in order.
pub(crate) fn masks_of(stage: &Stage, layer: &PrimPath) -> Vec<PrimPath> {
    let Ok(folder) = layer.child("masks") else {
        return Vec::new();
    };
    stage
        .children(&folder)
        .into_iter()
        .filter(|m| stage.prim(m).is_some_and(|v| v.is_a(&PrimType::MASK)))
        .collect()
}

fn is_mask(stage: &Stage, path: &PrimPath) -> bool {
    stage.prim(path).is_some_and(|v| v.is_a(&PrimType::MASK))
}

fn flag(stage: &Stage, mask: &PrimPath, attr: &str) -> bool {
    stage
        .prim(mask)
        .and_then(|v| query::static_value(&v, attr))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// The masks selected in the Timeline (their properties), in order.
fn selected_masks(stage: &Stage, session: &SharedSession) -> Vec<PrimPath> {
    let mut out: Vec<PrimPath> = Vec::new();
    for (path, _) in &session.lock().selected_props {
        if is_mask(stage, path) && !out.contains(path) {
            out.push(path.clone());
        }
    }
    out
}

/// The masks a Layer ▸ Mask command acts on: the selected masks, else every
/// mask of the selected layers; not those of locked layers.
pub(crate) fn targets(
    stage: &Stage,
    session: &SharedSession,
    layers: &[PrimPath],
) -> Vec<PrimPath> {
    let mut out = selected_masks(stage, session);
    if out.is_empty() {
        out = layers.iter().flat_map(|l| masks_of(stage, l)).collect();
    }
    out.retain(|m| query::locked_layer_of(stage, m).is_none());
    out
}

/// The first target of the selection (menu check marks).
pub(crate) fn first_target(stage: &Stage, session: &SharedSession) -> Option<PrimPath> {
    let layers = session.lock().selected_layers.clone();
    targets(stage, session, &layers).into_iter().next()
}

/// The mode of `mask` (`add` by default).
pub(crate) fn mode(stage: &Stage, mask: &PrimPath) -> String {
    stage
        .prim(mask)
        .and_then(|v| query::static_value(&v, "mode"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "add".to_owned())
}

pub(crate) fn inverted(stage: &Stage, mask: &PrimPath) -> bool {
    flag(stage, mask, "inverted")
}

pub(crate) fn locked(stage: &Stage, mask: &PrimPath) -> bool {
    flag(stage, mask, "locked")
}

/// A closed rectangle path through the box `[left, top, right, bottom]`,
/// clockwise from the top-left corner (AE's New Mask).
pub(crate) fn rect_path(b: [f64; 4]) -> BezierPath {
    let [l, t, r, bt] = b;
    BezierPath {
        vertices: vec![[l, t], [r, t], [r, bt], [l, bt]],
        in_tangents: vec![[0.0, 0.0]; 4],
        out_tangents: vec![[0.0, 0.0]; 4],
        closed: true,
        feather: Vec::new(),
    }
}

/// A closed ellipse path filling the box `[left, top, right, bottom]`.
pub(crate) fn ellipse_path(b: [f64; 4]) -> BezierPath {
    let [l, t, r, bt] = b;
    let (cx, cy) = ((l + r) / 2.0, (t + bt) / 2.0);
    let (rx, ry) = ((r - l) / 2.0, (bt - t) / 2.0);
    // The circle's Bezier handle length.
    const K: f64 = 0.552_284_75;
    let (kx, ky) = (rx * K, ry * K);
    BezierPath {
        vertices: vec![[cx, t], [r, cy], [cx, bt], [l, cy]],
        in_tangents: vec![[-kx, 0.0], [0.0, -ky], [kx, 0.0], [0.0, ky]],
        out_tangents: vec![[kx, 0.0], [0.0, ky], [-kx, 0.0], [0.0, -ky]],
        closed: true,
        feather: Vec::new(),
    }
}

/// The bounding box `[left, top, right, bottom]` of a path's vertices.
pub(crate) fn bounds(p: &BezierPath) -> Option<[f64; 4]> {
    let mut it = p.vertices.iter();
    let first = it.next()?;
    let mut b = [first[0], first[1], first[0], first[1]];
    for v in it {
        b = [
            b[0].min(v[0]),
            b[1].min(v[1]),
            b[2].max(v[0]),
            b[3].max(v[1]),
        ];
    }
    Some(b)
}

/// The content box of `layer` in layer space at comp time `now` (its source
/// size; the comp's size for layers without content).
pub(crate) fn layer_box(stage: &Stage, comp: &PrimPath, layer: &PrimPath, now: Time) -> [f64; 4] {
    let info = query::comp_info(stage, comp);
    let fallback = info.map_or([0.0, 0.0, 100.0, 100.0], |i| {
        [0.0, 0.0, f64::from(i.width), f64::from(i.height)]
    });
    let Ok(eval) = bae_anim::Evaluator::new().evaluate_comp(stage, comp, now) else {
        return fallback;
    };
    eval.layers
        .iter()
        .find(|l| l.path == *layer)
        .and_then(crate::render::layer_bounds)
        .map_or(fallback, |(a, b)| [a.x, a.y, b.x, b.y])
}

/// `attr.set_many` edits setting `attr` of each mask to `value(mask)` now.
fn edits(
    stage: &Stage,
    masks: &[PrimPath],
    now: Time,
    attr: &str,
    value: impl Fn(&PrimPath) -> Value,
) -> Vec<serde_json::Value> {
    masks
        .iter()
        .map(|m| {
            let time = query::to_key_time(stage, m, now);
            json!({"path": m, "attr": attr, "time": time, "value": value(m)})
        })
        .collect()
}

/// Handle a Layer ▸ Mask action in the active comp; `false` for other ids.
pub(crate) fn handle(
    id: &str,
    params: &serde_json::Value,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    comp: &PrimPath,
    layers: &[PrimPath],
    now: Time,
) -> bool {
    let stage = cx.stage;
    let masks = || targets(stage, session, layers);
    let set_many = |cx: &mut PanelContext<'_>, edits: Vec<serde_json::Value>| {
        if !edits.is_empty() {
            cx.invoke_with("attr.set_many", json!({ "edits": edits }));
        }
    };
    match id {
        ids::NEW_MASK => {
            // Layers that take masks (not cameras, lights, nulls or audio).
            let takes = |l: &PrimPath| {
                query::layer_kind(stage, l).is_some_and(|k| bae_motion::shape::takes_masks(&k))
            };
            for layer in layers
                .iter()
                .filter(|l| !query::is_locked(stage, l) && takes(l))
            {
                let [l, t, r, b] = layer_box(stage, comp, layer, now);
                cx.invoke_with(
                    "mask.create",
                    json!({
                        "layer": layer,
                        "kind": "path",
                        "path": rect_path([l, t, r, b]),
                    }),
                );
            }
        }
        ids::MASK_SHAPE | ids::MASK_FEATHER | ids::MASK_OPACITY | ids::MASK_EXPANSION => {
            let kind = match id {
                ids::MASK_SHAPE => crate::dialogs::MaskKind::Shape,
                ids::MASK_FEATHER => crate::dialogs::MaskKind::Feather,
                ids::MASK_OPACITY => crate::dialogs::MaskKind::Opacity,
                _ => crate::dialogs::MaskKind::Expansion,
            };
            let masks = masks();
            if !masks.is_empty() {
                crate::dialogs::mask_value(cx, comp, &masks, now, kind);
            }
        }
        ids::RESET_MASK => {
            let masks = masks();
            let e = edits(stage, &masks, now, "path", |m| {
                let layer = query::owning_layer(stage, m).unwrap_or_else(|| m.clone());
                Value::Bezier(rect_path(layer_box(stage, comp, &layer, now)))
            });
            set_many(cx, e);
        }
        ids::REMOVE_MASK | ids::REMOVE_ALL_MASKS => {
            let masks: Vec<PrimPath> = if id == ids::REMOVE_MASK {
                masks()
            } else {
                layers
                    .iter()
                    .filter(|l| !query::is_locked(stage, l))
                    .flat_map(|l| masks_of(stage, l))
                    .collect()
            };
            if !masks.is_empty() {
                cx.invoke_with("item.delete", json!({ "paths": masks }));
            }
        }
        ids::MASK_MODE => {
            let Some(mode) = params.get("mode").and_then(|v| v.as_str()) else {
                return true;
            };
            let e = edits(stage, &masks(), now, "mode", |_| Value::Enum(mode.into()));
            set_many(cx, e);
        }
        ids::MASK_INVERTED | ids::MASK_LOCKED => {
            let attr = if id == ids::MASK_INVERTED {
                "inverted"
            } else {
                "locked"
            };
            let masks = masks();
            // AE: on unless every target already has it.
            let on = !masks.iter().all(|m| flag(stage, m, attr));
            let e = edits(stage, &masks, now, attr, |_| Value::Bool(on));
            set_many(cx, e);
        }
        ids::UNLOCK_ALL_MASKS | ids::LOCK_OTHER_MASKS => {
            let all: Vec<PrimPath> = layers.iter().flat_map(|l| masks_of(stage, l)).collect();
            let selected = selected_masks(stage, session);
            let (on, which): (bool, Vec<PrimPath>) = if id == ids::UNLOCK_ALL_MASKS {
                (
                    false,
                    all.into_iter().filter(|m| locked(stage, m)).collect(),
                )
            } else {
                (
                    true,
                    all.into_iter()
                        .filter(|m| !selected.contains(m) && !locked(stage, m))
                        .collect(),
                )
            };
            let e = edits(stage, &which, now, "locked", |_| Value::Bool(on));
            set_many(cx, e);
        }
        _ => return false,
    }
    true
}
