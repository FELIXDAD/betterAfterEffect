//! Layer ▸ Mask and Shape Path (AE 26): RotoBezier, Closed and Set First Vertex
//! on the selected masks or shape Paths, in every keyframe (one undo step,
//! `path.edit_whole`). Free Transform Points is the viewer's (`tools`).

use bae_model::{BezierPath, PrimPath, PrimType, Stage, Value};
use bae_motion::query;
use bae_ui::PanelContext;
use bae_ui::menu::MenuContext;
use serde_json::json;

use crate::session::SharedSession;

pub(crate) const ROTOBEZIER: &str = "motion.layer.rotobezier";
pub(crate) const CLOSED: &str = "motion.layer.path_closed";
pub(crate) const SET_FIRST_VERTEX: &str = "motion.layer.set_first_vertex";

/// `(id, English title)`, for the shortcut editor.
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    (ROTOBEZIER, "RotoBezier"),
    (CLOSED, "Closed"),
    (SET_FIRST_VERTEX, "Set First Vertex"),
];

fn is_path(stage: &Stage, p: &PrimPath) -> bool {
    stage
        .prim(p)
        .is_some_and(|v| v.is_a(&PrimType::MASK) || v.is_a(&PrimType::PATH))
}

/// The masks and shape Paths the menu acts on: the one whose vertices the
/// Composition viewer has selected, and the paths, masks and Path rows
/// selected in the Timeline.
pub(crate) fn targets(stage: &Stage, session: &SharedSession) -> Vec<PrimPath> {
    let s = session.lock();
    let mut out: Vec<PrimPath> = Vec::new();
    let candidates = s
        .vertex_path
        .iter()
        .cloned()
        .chain(
            s.selected_props
                .iter()
                .filter(|(_, attr)| attr == "path")
                .map(|(p, _)| p.clone()),
        )
        .chain(
            s.selected_groups
                .iter()
                .filter_map(|id| crate::session::group_prim(id)),
        );
    for p in candidates {
        if is_path(stage, &p) && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The path's shape: its static value, or its first keyframe's.
fn shape(stage: &Stage, p: &PrimPath) -> Option<BezierPath> {
    let spec = query::attr_spec(stage, p, "path")?;
    let v = spec
        .value
        .as_ref()
        .or_else(|| spec.keyframes.values().next().map(|k| &k.value))?;
    match v {
        Value::Bezier(b) => Some(b.clone()),
        _ => None,
    }
}

fn closed(stage: &Stage, p: &PrimPath) -> bool {
    shape(stage, p).is_some_and(|b| b.closed)
}

/// The one vertex selected in the viewer: its path and index.
fn single_vertex(session: &SharedSession) -> Option<(PrimPath, usize)> {
    let s = session.lock();
    match (&s.vertex_path, s.vertex_indices.as_slice()) {
        (Some(p), [i]) => Some((p.clone(), *i)),
        _ => None,
    }
}

fn with_targets<T>(cx: &MenuContext<'_>, f: impl FnOnce(&[PrimPath]) -> T) -> T {
    let session = crate::common::session(cx.resources);
    f(&targets(cx.stage, &session))
}

pub(crate) fn any_target(cx: &MenuContext<'_>) -> bool {
    with_targets(cx, |t| !t.is_empty())
}

pub(crate) fn all_roto(cx: &MenuContext<'_>) -> bool {
    with_targets(cx, |t| {
        !t.is_empty()
            && t.iter()
                .all(|p| bae_motion::path::is_roto_bezier(cx.stage, p))
    })
}

/// Closed closes open paths; a closed one shows checked and greyed (AE 26,
/// #591's tester data).
pub(crate) fn any_open(cx: &MenuContext<'_>) -> bool {
    with_targets(cx, |t| t.iter().any(|p| !closed(cx.stage, p)))
}

pub(crate) fn all_closed(cx: &MenuContext<'_>) -> bool {
    with_targets(cx, |t| {
        !t.is_empty() && t.iter().all(|p| closed(cx.stage, p))
    })
}

pub(crate) fn one_vertex(cx: &MenuContext<'_>) -> bool {
    single_vertex(&crate::common::session(cx.resources)).is_some()
}

/// Runs a Mask and Shape Path item; `false` for other actions.
pub(crate) fn handle(id: &str, cx: &mut PanelContext<'_>) -> bool {
    let session = crate::common::session(cx.resources);
    let stage = cx.stage;
    let params = match id {
        ROTOBEZIER => {
            let t = targets(stage, &session);
            let on = !t.iter().all(|p| bae_motion::path::is_roto_bezier(stage, p));
            (!t.is_empty()).then(|| json!({"paths": t, "edit": "roto_bezier", "on": on}))
        }
        CLOSED => {
            let t: Vec<PrimPath> = targets(stage, &session)
                .into_iter()
                .filter(|p| !closed(stage, p))
                .collect();
            (!t.is_empty()).then(|| json!({"paths": t, "edit": "closed", "closed": true}))
        }
        SET_FIRST_VERTEX => single_vertex(&session).map(|(p, i)| {
            // The selected vertex is vertex 1 now.
            session.lock().vertex_request = Some(vec![0]);
            json!({"paths": [p], "edit": "first_vertex", "index": i})
        }),
        _ => return false,
    };
    if let Some(params) = params {
        cx.invoke_with("path.edit_whole", params);
    }
    true
}
