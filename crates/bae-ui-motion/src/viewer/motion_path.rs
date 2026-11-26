//! Motion paths in the Composition panel (AE): for each selected layer whose
//! Position is animated, the path it travels — a dot per frame (their spacing shows
//! the speed), a square per keyframe (filled when the keyframe is selected, in sync
//! with the Timeline) and the spatial Bezier handles.
//!
//! With the Selection tool: click a keyframe square to select the keyframe (Shift
//! adds or removes); drag it to change that keyframe's value; drag a handle to
//! change its spatial tangent (the other handle of a continuous keyframe turns with
//! it; Cmd/Ctrl-drag breaks them apart); Cmd/Ctrl-drag a keyframe square to pull new
//! handles out of it. Each drag is one `key.edit`.
//!
//! With the Convert Vertex tool: a click on a square toggles the keyframe
//! between linear (no handles) and Auto Bezier; a drag pulls new handles out
//! of it (AE).
//!
//! Preferences > Appearance > Motion Path decides how much of the path shows: none,
//! all keyframes, no more than N keyframes or no more than a duration around the
//! current time.

use bae_anim::glam::{DMat4, DVec3};
use bae_anim::{Curve, CurveKind, EvaluatedComp};
use bae_model::{AttrSpec, PrimPath, Time};
use bae_motion::query::{self, CompInfo};
use bae_motion::{KeyEdit, KeyRef};
use bae_ui::PanelContext;
use egui::{Color32, CursorIcon, Pos2, Rect, Response, Stroke, Ui, vec2};
use serde_json::json;

use super::prefs::ViewerPrefs;
use crate::graph::{comps, recompose};
use crate::render::View;
use crate::session::SharedSession;
use crate::tools::Tool;

const POSITION: &str = "transform:position";
/// Pick distance for squares and handles (points).
const PICK: f32 = 6.0;

/// Preferences > Appearance > Motion Path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PathMode {
    None,
    All,
    /// No more than N keyframes around the current time.
    Keys(usize),
    /// No more than this duration around the current time.
    Time(Time),
}

impl PathMode {
    pub(crate) fn current(resources: &bae_app::Resources) -> Self {
        let store = resources.get::<std::sync::Arc<bae_prefs::PrefsStore>>();
        let prefs = store.map_or_else(
            || std::sync::Arc::new(bae_prefs::Preferences::default()),
            |s| s.current(),
        );
        match prefs.str(bae_prefs::keys::MOTION_PATH_MODE) {
            "no_motion_path" => PathMode::None,
            "all_keyframes" => PathMode::All,
            "max_keyframes" => {
                PathMode::Keys(prefs.int("appearance.motion_path_max_keyframes").max(1) as usize)
            }
            _ => {
                let window = prefs
                    .get("appearance.motion_path_time_window")
                    .and_then(|v| v.as_str())
                    .and_then(bae_prefs::parse_timecode)
                    .map_or(15.0, |[h, m, s, f]| {
                        // Frames at 30 fps until the comp's rate is known here.
                        f64::from(h * 3600 + m * 60 + s) + f64::from(f) / 30.0
                    });
                PathMode::Time(Time::from_seconds(window))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum PathDrag {
    /// A keyframe square: its value follows the pointer.
    Key { key: KeyRef, origin: Pos2 },
    /// A handle (`out`: the outgoing one); `split`: Cmd/Ctrl held at the press, the
    /// other handle stays.
    Handle { key: KeyRef, out: bool, split: bool },
}

/// Motion-path state of a Composition panel.
#[derive(Default)]
pub(crate) struct MotionPathState {
    drag: Option<PathDrag>,
    /// The primary button went down on a square or handle (what it hit, where) and
    /// is still down: the gesture is the motion path's from the press on (the
    /// Selection tool reacts at mouse-down).
    pressed: Option<(KeyRef, Option<bool>, Pos2)>,
    /// Last frame's geometry, for tests.
    #[cfg(test)]
    pub(crate) last: Drawn,
}

/// What a frame drew (screen positions).
#[derive(Default, Clone, Debug)]
pub(crate) struct Drawn {
    pub keys: Vec<(KeyRef, Pos2)>,
    /// (keyframe, outgoing, handle position)
    pub handles: Vec<(KeyRef, bool, Pos2)>,
    pub dots: Vec<Pos2>,
}

/// What the motion path did this frame.
pub(crate) struct PathFrame {
    /// The press, click or drag is the motion path's: the Selection tool must not
    /// also act on it.
    pub owns: bool,
    pub cursor: Option<CursorIcon>,
}

/// One layer's motion path.
struct LayerPath {
    layer: PrimPath,
    spec: AttrSpec,
    /// Parent space (Position values) → comp space.
    to_comp: DMat4,
    color: Color32,
}

impl LayerPath {
    fn screen(&self, view: &View, p: DVec3) -> Pos2 {
        let c = self.to_comp.transform_point3(p);
        view.to_screen(bae_anim::glam::DVec2::new(c.x, c.y))
    }

    /// Pointer → parent space.
    fn parent_point(&self, view: &View, pos: Pos2) -> DVec3 {
        let c = view.to_comp(pos);
        self.to_comp
            .inverse()
            .transform_point3(DVec3::new(c.x, c.y, 0.0))
    }
}

fn vec3(v: &[f64]) -> DVec3 {
    DVec3::new(
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
        v.get(2).copied().unwrap_or(0.0),
    )
}

/// The keyframes of `spec` shown under `mode` with the current keyframe time `now`.
fn shown_keys(spec: &AttrSpec, mode: PathMode, now: Time, rate: f64) -> Vec<Time> {
    let all: Vec<Time> = spec.keyframes.keys().copied().collect();
    match mode {
        PathMode::None => Vec::new(),
        PathMode::All => all,
        PathMode::Keys(n) => {
            // The n keyframes around the current time.
            let at = all.iter().position(|t| *t >= now).unwrap_or(all.len());
            let start = at.saturating_sub(n / 2).min(all.len().saturating_sub(n));
            all.into_iter().skip(start).take(n).collect()
        }
        PathMode::Time(w) => {
            let half = Time((w.0 as f64 / 2.0 * rate.max(1e-9)) as i64);
            all.into_iter()
                .filter(|t| (*t - now).0.abs() <= half.0)
                .collect()
        }
    }
}

/// The paths of the selected layers (`edit` applied as a drag preview).
fn layer_paths(
    cx: &PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    prefs: &ViewerPrefs,
    edit: Option<&KeyEdit>,
) -> Vec<LayerPath> {
    let stage = cx.stage;
    let selected = session.lock().selected_layers.clone();
    let mut out = Vec::new();
    for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        let Some(spec) = query::attr_spec(stage, &l.path, POSITION) else {
            continue;
        };
        if spec.keyframes.is_empty() || bae_motion::dimensions::is_separated(stage, &l.path) {
            continue;
        }
        let mut spec = spec.clone();
        if let Some(e) = edit.filter(|e| e.key.path == l.path && e.key.attr == POSITION) {
            spec = bae_motion::apply_key_edits(POSITION, &spec, &[e]).0;
        }
        let to_comp = l
            .parent
            .and_then(|i| comp.layers.get(i))
            .map_or(DMat4::IDENTITY, |p| p.world_matrix);
        let label = stage
            .prim(&l.path)
            .and_then(|v| query::static_value(&v, "label"))
            .and_then(|v| v.as_f64())
            .map_or(8, |v| v as usize);
        out.push(LayerPath {
            layer: l.path.clone(),
            spec,
            to_comp,
            color: prefs.handle_color(bae_ui::theme::label_color(label)),
        });
    }
    out
}

/// Geometry of the paths: dots, keyframe squares and handles on screen.
fn geometry(
    cx: &PanelContext<'_>,
    info: &CompInfo,
    view: &View,
    paths: &[LayerPath],
    mode: PathMode,
    now: Time,
) -> Vec<(usize, Drawn)> {
    let stage = cx.stage;
    let mut out = Vec::new();
    for (pi, p) in paths.iter().enumerate() {
        let rate = query::layer_timing(stage, &p.layer).stretch.abs().max(1.0) / 100.0;
        let key_now = query::to_key_time(stage, &p.layer, now);
        let times = shown_keys(&p.spec, mode, key_now, 1.0 / rate);
        let (Some(first), Some(last)) = (times.first().copied(), times.last().copied()) else {
            continue;
        };
        let Some(curve) = Curve::new(&p.spec.keyframes, CurveKind::Spatial) else {
            continue;
        };
        let mut d = Drawn::default();
        // A dot per comp frame between the first and last shown keyframes.
        let (ca, cb) = (
            query::to_comp_time(stage, &p.layer, first),
            query::to_comp_time(stage, &p.layer, last),
        );
        let (ca, cb) = (ca.min(cb), ca.max(cb));
        let mut f = info.frame_rate.time_to_frame(ca);
        loop {
            let ct = info.frame_rate.frame_to_time(f);
            if ct > cb {
                break;
            }
            if ct >= ca {
                let kt = query::to_key_time(stage, &p.layer, ct);
                if let Some(v) = comps(&curve.sample(kt)) {
                    d.dots.push(p.screen(view, vec3(&v)));
                }
            }
            f += 1;
            if f - info.frame_rate.time_to_frame(ca) > 100_000 {
                break;
            }
        }
        for t in &times {
            let Some(k) = p.spec.keyframes.get(t) else {
                continue;
            };
            let Some(v) = comps(&k.value) else {
                continue;
            };
            let at = vec3(&v);
            let kr = KeyRef::new(p.layer.clone(), POSITION, *t);
            d.keys.push((kr.clone(), p.screen(view, at)));
            for (out, tangent) in [(false, k.spatial_in), (true, k.spatial_out)] {
                if let Some(tg) = tangent.filter(|tg| tg.iter().any(|c| c.abs() > 1e-9)) {
                    d.handles
                        .push((kr.clone(), out, p.screen(view, at + DVec3::from_array(tg))));
                }
            }
        }
        out.push((pi, d));
    }
    out
}

/// The edit a drag makes with the pointer at `pointer`.
fn drag_edit(drag: &PathDrag, paths: &[LayerPath], view: &View, pointer: Pos2) -> Option<KeyEdit> {
    let (key, path) = match drag {
        PathDrag::Key { key, .. } | PathDrag::Handle { key, .. } => {
            (key, paths.iter().find(|p| p.layer == key.path)?)
        }
    };
    let kf = path.spec.keyframes.get(&key.time)?;
    let at = vec3(&comps(&kf.value)?);
    let mut e = KeyEdit::new(key.clone());
    match drag {
        PathDrag::Key { origin, .. } => {
            let delta = path.parent_point(view, pointer) - path.parent_point(view, *origin);
            let v = comps(&kf.value)?;
            let moved: Vec<f64> = v
                .iter()
                .enumerate()
                .map(|(i, c)| c + [delta.x, delta.y, 0.0].get(i).copied().unwrap_or(0.0))
                .collect();
            e.value = Some(recompose(&kf.value, &moved));
        }
        PathDrag::Handle { out, split, .. } => {
            let t = path.parent_point(view, pointer) - at;
            let t = DVec3::new(t.x, t.y, 0.0);
            let other = if *out { kf.spatial_in } else { kf.spatial_out };
            let mirrored = if *split {
                other
            } else {
                // A continuous keyframe: the other handle turns with this one and
                // keeps its length (or mirrors this one when it had none).
                let len = other.map_or(0.0, |o| DVec3::from_array(o).length());
                let len = if len > 1e-9 { len } else { t.length() };
                let dir = if t.length() > 1e-9 {
                    -t.normalize()
                } else {
                    DVec3::ZERO
                };
                Some((dir * len).to_array())
            };
            let (tin, tout) = if *out {
                (mirrored, Some(t.to_array()))
            } else {
                (Some(t.to_array()), mirrored)
            };
            e.spatial_in = tin;
            e.spatial_out = tout;
            e.spatial_continuous = Some(!*split);
            e.spatial_auto_bezier = Some(false);
        }
    }
    Some(e)
}

/// Convert Vertex on a keyframe square: handles retract (linear), or a linear
/// keyframe becomes Auto Bezier.
fn convert_edit(key: &KeyRef, paths: &[LayerPath]) -> Option<KeyEdit> {
    let path = paths.iter().find(|p| p.layer == key.path)?;
    let kf = path.spec.keyframes.get(&key.time)?;
    let has = |t: Option<[f64; 3]>| t.is_some_and(|t| DVec3::from_array(t).length() > 1e-9);
    let curved = kf.flags.spatial_auto_bezier || has(kf.spatial_in) || has(kf.spatial_out);
    let mut e = KeyEdit::new(key.clone());
    if curved {
        e.spatial_in = Some([0.0; 3]);
        e.spatial_out = Some([0.0; 3]);
        e.spatial_continuous = Some(false);
        e.spatial_auto_bezier = Some(false);
    } else {
        e.spatial_continuous = Some(true);
        e.spatial_auto_bezier = Some(true);
    }
    Some(e)
}

/// Draw the motion paths of the selected layers and, with the Selection tool, edit
/// them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn frame(
    state: &mut MotionPathState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &egui::Painter,
    prefs: &ViewerPrefs,
) -> PathFrame {
    let mode = PathMode::current(cx.resources);
    #[cfg(test)]
    {
        state.last = Drawn::default();
    }
    let mut out = PathFrame {
        owns: state.drag.is_some() || state.pressed.is_some(),
        cursor: None,
    };
    // View Options: Motion Paths, Keyframes, Motion Path Tangents.
    let shown = session.lock().layer_controls;
    if mode == PathMode::None || !shown.motion_paths {
        state.drag = None;
        state.pressed = None;
        return out;
    }
    let now = session.lock().time(&info.path);
    let pointer = ui.input(|i| i.pointer.interact_pos());
    // Drag preview: the edit the drag would make now, applied locally.
    let base = layer_paths(cx, session, comp, prefs, None);
    let preview = match (&state.drag, pointer) {
        (Some(d), Some(p)) => drag_edit(d, &base, view, p),
        _ => None,
    };
    let paths = if preview.is_some() {
        layer_paths(cx, session, comp, prefs, preview.as_ref())
    } else {
        base
    };
    let mut drawn = geometry(cx, info, view, &paths, mode, now);
    for (_, d) in &mut drawn {
        if !shown.tangents {
            d.handles.clear();
        }
        if !shown.keyframes {
            d.keys.clear();
        }
    }
    let selected = session.lock().selected_keys.clone();

    // Paint: dots, handles and their lines, squares (filled when selected).
    let size = prefs.point_size.max(4.0);
    for (pi, d) in &drawn {
        let c = paths[*pi].color;
        for p in &d.dots {
            painter.circle_filled(*p, 1.5, c);
        }
        for (k, _, h) in &d.handles {
            if let Some((_, at)) = d.keys.iter().find(|(x, _)| x == k) {
                painter.line_segment([*at, *h], Stroke::new(1.0, c));
            }
            painter.circle_filled(*h, size * 0.45, c);
        }
        for (k, at) in &d.keys {
            let r = Rect::from_center_size(*at, vec2(size, size));
            if selected.contains(k) {
                painter.rect_filled(r, 0.0, c);
            } else {
                painter.rect_filled(r, 0.0, Color32::from_gray(30));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, c), egui::StrokeKind::Middle);
            }
        }
    }
    #[cfg(test)]
    {
        for (_, d) in &drawn {
            state.last.keys.extend(d.keys.iter().cloned());
            state.last.handles.extend(d.handles.iter().cloned());
            state.last.dots.extend(d.dots.iter().copied());
        }
    }

    let converting = tool == Tool::ConvertVertex;
    if tool != Tool::Selection && !converting {
        state.drag = None;
        state.pressed = None;
        out.owns = false;
        return out;
    }
    let hit = |pos: Pos2| -> Option<(KeyRef, Option<bool>)> {
        let near = |p: &Pos2| p.distance(pos) <= PICK.max(size);
        for (_, d) in &drawn {
            if let Some((k, o, _)) = d.handles.iter().find(|(_, _, h)| near(h)) {
                return Some((k.clone(), Some(*o)));
            }
        }
        // The square at the current time sits on the layer itself: dragging there
        // moves the layer with the Selection tool (same value change, and every
        // selected layer moves along).
        let at_cti = |k: &KeyRef| query::to_comp_time(cx.stage, &k.path, k.time) == now;
        for (_, d) in &drawn {
            if let Some((k, _)) = d.keys.iter().find(|(k, p)| near(p) && !at_cti(k)) {
                return Some((k.clone(), None));
            }
        }
        None
    };
    let mods = ui.input(|i| i.modifiers);
    if state.drag.is_none()
        && let Some(pos) = resp.hover_pos()
        && hit(pos).is_some()
    {
        out.cursor = Some(CursorIcon::Default);
    }
    let press = ui.input(|i| i.pointer.press_origin());
    // A press on a square or handle: ours until the button comes up.
    if ui.input(|i| i.pointer.primary_pressed())
        && let Some(pos) = press
        && resp.hovered()
        && let Some((k, handle)) = hit(pos)
    {
        state.pressed = Some((k, handle, pos));
        out.owns = true;
    }
    // Convert Vertex: a click toggles linear / Auto Bezier (one key.edit).
    if converting
        && resp.clicked_by(egui::PointerButton::Primary)
        && let Some((k, None, _)) = state.pressed.clone()
    {
        out.owns = true;
        if query::locked_layer_of(cx.stage, &k.path).is_none()
            && let Some(e) = convert_edit(&k, &paths)
        {
            cx.invoke_with("key.edit", json!({ "edits": [e] }));
        }
    } else if resp.clicked_by(egui::PointerButton::Primary)
        && let Some((k, None, _)) = state.pressed.clone()
    {
        // Clicks select keyframes (Shift adds or removes).
        out.owns = true;
        let mut s = session.lock();
        if mods.shift {
            if let Some(i) = s.selected_keys.iter().position(|x| *x == k) {
                s.selected_keys.remove(i);
            } else {
                s.selected_keys.push(k);
            }
        } else {
            s.selected_keys = vec![k];
        }
    }
    // Drag start on a square or handle.
    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some((k, handle, pos)) = state.pressed.clone()
    {
        out.owns = true;
        if query::locked_layer_of(cx.stage, &k.path).is_none() {
            state.drag = Some(match handle {
                // Convert Vertex drags one handle on its own (AE).
                Some(o) => PathDrag::Handle {
                    key: k,
                    out: o,
                    split: mods.command || converting,
                },
                // Cmd/Ctrl-drag (or Convert Vertex) on a square pulls out new
                // handles.
                None if mods.command || converting => PathDrag::Handle {
                    key: k,
                    out: true,
                    split: false,
                },
                None => {
                    let mut s = session.lock();
                    if !s.selected_keys.contains(&k) {
                        s.selected_keys = vec![k.clone()];
                    }
                    PathDrag::Key {
                        key: k,
                        origin: pos,
                    }
                }
            });
        }
    }
    // Release: one key.edit.
    if state.drag.is_some() && ui.input(|i| i.pointer.any_released()) {
        let edit = match (&state.drag, pointer) {
            (Some(d), Some(p)) => {
                let base = layer_paths(cx, session, comp, prefs, None);
                drag_edit(d, &base, view, p)
            }
            _ => None,
        };
        state.drag = None;
        out.owns = true;
        if let Some(e) = edit {
            cx.invoke_with("key.edit", json!({ "edits": [e] }));
        }
    }
    // The gesture ends with the button (after its click was handled above).
    if !ui.input(|i| i.pointer.primary_down()) && state.drag.is_none() {
        state.pressed = None;
    }
    out
}

#[cfg(test)]
mod tests {
    use bae_anim::glam::DVec2;
    use bae_app::Resources;
    use bae_model::{AuthorId, CommandRegistry, Stage, Value};
    use bae_ui::{Panel, PanelContext, PanelId, PanelRequests};
    use egui::{Event, Modifiers, PointerButton, RawInput};
    use serde_json::Value as Json;

    use super::*;
    use crate::session::SharedSession;
    use crate::tools::SharedTools;
    use crate::viewer::CompositionViewer;

    const SEC: i64 = Time::FLICKS_PER_SECOND;

    struct H {
        stage: Stage,
        reg: CommandRegistry,
        author: AuthorId,
        resources: Resources,
        ctx: egui::Context,
        viewer: CompositionViewer,
        layer: PrimPath,
        time: f64,
        mods: Modifiers,
    }

    impl H {
        /// A 640×360 comp with a solid whose Position has keyframes at 0, 1 and
        /// 2 s ((100, 100), (300, 100), (300, 250)), selected; the CTI at 0.5 s.
        fn new() -> Self {
            let mut reg = CommandRegistry::with_builtins();
            bae_motion::register_commands(&mut reg);
            let mut resources = Resources::new();
            resources.insert(SharedSession::default());
            resources.insert(SharedTools::default());
            let mut h = H {
                stage: Stage::new(),
                reg,
                author: AuthorId::new("user:test").unwrap(),
                resources,
                ctx: bae_ui::testing::ctx_with_fonts(),
                viewer: CompositionViewer::new(None),
                layer: PrimPath::new("/x").unwrap(),
                time: 0.0,
                mods: Modifiers::NONE,
            };
            let comp = h.run(
                "comp.create",
                json!({"width": 640, "height": 360, "fps": 30}),
            )["path"]
                .clone();
            let comp = PrimPath::new(comp.as_str().unwrap()).unwrap();
            h.layer = PrimPath::new(
                h.run(
                    "layer.create",
                    json!({"comp": comp, "kind": "solid", "size": [20, 20]}),
                )["path"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            let l = h.layer.clone();
            h.run(
                "attr.set_animated",
                json!({"path": l, "attr": POSITION, "animated": true, "time": 0}),
            );
            for (t, x, y) in [
                (0, 100.0, 100.0),
                (SEC, 300.0, 100.0),
                (2 * SEC, 300.0, 250.0),
            ] {
                h.run(
                    "attr.set_at",
                    json!({"path": l, "attr": POSITION, "time": t, "value": {"type": "vec3", "value": [x, y, 0.0]}}),
                );
            }
            // The CTI between keyframes: the square at the current time belongs to
            // the Selection tool.
            let info = query::comp_info(&h.stage, &comp).unwrap();
            {
                let s = h.session();
                let mut s = s.lock();
                s.open_comp(&comp);
                s.selected_layers = vec![l];
                s.set_time(&info, Time::from_seconds(0.5));
            }
            h.frame(Vec::new(), Modifiers::NONE);
            h.frame(Vec::new(), Modifiers::NONE);
            h
        }

        fn session(&self) -> SharedSession {
            self.resources.get::<SharedSession>().unwrap().clone()
        }

        fn run(&mut self, id: &str, params: Json) -> Json {
            self.reg
                .execute(&mut self.stage, &self.author, id, params)
                .unwrap_or_else(|e| panic!("{id}: {e}"))
                .output
        }

        fn frame(&mut self, mut events: Vec<Event>, modifiers: Modifiers) -> PanelRequests {
            self.time += 1.0 / 60.0;
            if modifiers != self.mods {
                self.mods = modifiers;
                events.insert(0, Event::ModifiersChanged(modifiers));
            }
            let mut requests = PanelRequests::default();
            let id = PanelId::new("composition");
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                time: Some(self.time),
                predicted_dt: 1.0 / 60.0,
                events,
                ..Default::default()
            };
            let (stage, author, resources) = (&self.stage, &self.author, &self.resources);
            let viewer = &mut self.viewer;
            let mut out = self.ctx.run_ui(raw, |ui| {
                let mut cx =
                    PanelContext::new(&id, "Composition", stage, author, resources, &mut requests);
                cx.focused = true;
                viewer.ui(ui, &mut cx);
            });
            out.textures_delta.clear();
            requests
        }

        fn apply(&mut self, r: &PanelRequests) {
            for inv in &r.invocations {
                self.reg
                    .execute(&mut self.stage, &self.author, &inv.id, inv.params.clone())
                    .unwrap_or_else(|e| panic!("{}: {e}", inv.id));
            }
            self.frame(Vec::new(), Modifiers::NONE);
        }

        fn button(&mut self, p: Pos2, pressed: bool, m: Modifiers) -> PanelRequests {
            self.frame(
                vec![Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: m,
                }],
                m,
            )
        }

        /// Press at `a`, move to `b` in steps, release.
        fn drag(&mut self, a: Pos2, b: Pos2, m: Modifiers) -> PanelRequests {
            self.frame(vec![Event::PointerMoved(a)], m);
            self.button(a, true, m);
            for i in 1..=6 {
                let p = a + (b - a) * (i as f32 / 6.0);
                let r = self.frame(vec![Event::PointerMoved(p)], m);
                assert!(r.invocations.is_empty(), "no command while dragging");
            }
            self.button(b, false, m)
        }

        fn click(&mut self, p: Pos2, m: Modifiers) {
            self.frame(vec![Event::PointerMoved(p)], m);
            self.button(p, true, m);
            self.button(p, false, m);
        }

        fn drawn(&self) -> &Drawn {
            &self.viewer.motion_path.last
        }

        fn key(&self, t: i64) -> KeyRef {
            KeyRef::new(self.layer.clone(), POSITION, Time(t))
        }

        fn key_pos(&self, t: i64) -> Pos2 {
            let k = self.key(t);
            self.drawn()
                .keys
                .iter()
                .find(|(x, _)| *x == k)
                .map(|(_, p)| *p)
                .unwrap_or_else(|| panic!("key at {t} not drawn"))
        }

        /// Comp pixels → screen points.
        fn scale(&self) -> f32 {
            self.viewer.last_view.unwrap().scale
        }

        fn kf(&self, t: i64) -> bae_model::Keyframe {
            query::attr_spec(&self.stage, &self.layer, POSITION)
                .unwrap()
                .keyframes[&Time(t)]
                .clone()
        }
    }

    /// The Convert Vertex tool on a square: linear → Auto Bezier → linear.
    #[test]
    fn convert_vertex_toggles_a_keyframe_between_linear_and_auto_bezier() {
        let mut h = H::new();
        h.resources
            .get::<SharedTools>()
            .unwrap()
            .lock()
            .set(Tool::ConvertVertex);
        let click = |h: &mut H| {
            let p = h.key_pos(SEC);
            h.frame(vec![Event::PointerMoved(p)], Modifiers::NONE);
            h.button(p, true, Modifiers::NONE);
            let r = h.button(p, false, Modifiers::NONE);
            assert_eq!(r.invocations.len(), 1, "one key.edit");
            assert_eq!(r.invocations[0].id, "key.edit");
            h.apply(&r);
            // Not a double-click next time.
            for _ in 0..40 {
                h.frame(Vec::new(), Modifiers::NONE);
            }
        };
        click(&mut h);
        assert!(h.kf(SEC).flags.spatial_auto_bezier, "Auto Bezier");
        click(&mut h);
        let k = h.kf(SEC);
        assert!(!k.flags.spatial_auto_bezier, "linear again");
        let zero = |t: Option<[f64; 3]>| t.is_none_or(|t| t == [0.0; 3]);
        assert!(zero(k.spatial_in) && zero(k.spatial_out), "{k:?}");
    }

    #[test]
    fn draws_a_square_per_keyframe_and_a_dot_per_frame() {
        let h = H::new();
        assert_eq!(h.drawn().keys.len(), 3);
        // 0 .. 2 s at 30 fps: 61 frames.
        assert_eq!(h.drawn().dots.len(), 61);
        // The square sits where the comp point maps on screen.
        let view = h.viewer.last_view.unwrap();
        let want = view.to_screen(DVec2::new(300.0, 100.0));
        assert!(h.key_pos(SEC).distance(want) < 0.5);
        // Linear keyframes have no handles.
        assert!(h.drawn().handles.is_empty());
    }

    #[test]
    fn clicking_a_square_selects_the_keyframe_like_the_timeline() {
        let mut h = H::new();
        let p = h.key_pos(SEC);
        h.click(p, Modifiers::NONE);
        assert_eq!(h.session().lock().selected_keys, vec![h.key(SEC)]);
        assert_eq!(
            h.session().lock().selected_layers,
            vec![h.layer.clone()],
            "the layer stays selected"
        );
        // Shift adds.
        let p0 = h.key_pos(0);
        h.click(p0, Modifiers::SHIFT);
        assert_eq!(h.session().lock().selected_keys, vec![h.key(SEC), h.key(0)]);
    }

    #[test]
    fn dragging_a_square_moves_that_keyframe_in_one_step() {
        let mut h = H::new();
        let from = h.key_pos(SEC);
        let s = h.scale();
        let r = h.drag(from, from + vec2(50.0 * s, 20.0 * s), Modifiers::NONE);
        assert_eq!(r.invocations.len(), 1);
        assert_eq!(r.invocations[0].id, "key.edit");
        h.apply(&r);
        let Value::Vec3(v) = h.kf(SEC).value else {
            panic!("vec3")
        };
        assert!(
            (v[0] - 350.0).abs() < 0.5 && (v[1] - 120.0).abs() < 0.5,
            "{v:?}"
        );
        assert_eq!(h.session().lock().selected_keys, vec![h.key(SEC)]);
        // The layer itself did not move in time and the other keys stayed.
        assert_eq!(h.kf(0).value, Value::Vec3([100.0, 100.0, 0.0]));
        h.stage.undo(&h.author).unwrap();
        assert_eq!(h.kf(SEC).value, Value::Vec3([300.0, 100.0, 0.0]));
    }

    #[test]
    fn cmd_drag_pulls_out_handles_and_handles_drag_together_or_apart() {
        let mut h = H::new();
        let cmd = Modifiers {
            command: true,
            mac_cmd: cfg!(target_os = "macos"),
            ctrl: !cfg!(target_os = "macos"),
            ..Modifiers::NONE
        };
        let s = h.scale();
        // Cmd/Ctrl-drag on the middle square: out handle at (+40, 0), in mirrored.
        let from = h.key_pos(SEC);
        let r = h.drag(from, from + vec2(40.0 * s, 0.0), cmd);
        h.apply(&r);
        let k = h.kf(SEC);
        let out = k.spatial_out.unwrap();
        let tin = k.spatial_in.unwrap();
        assert!((out[0] - 40.0).abs() < 0.5 && out[1].abs() < 0.5, "{out:?}");
        assert!((tin[0] + 40.0).abs() < 0.5, "{tin:?}");
        assert!(k.flags.spatial_continuous);
        assert_eq!(h.drawn().handles.len(), 2);
        // Plain drag of the out handle up: the in handle turns with it.
        let hp = h
            .drawn()
            .handles
            .iter()
            .find(|(_, o, _)| *o)
            .map(|(_, _, p)| *p)
            .unwrap();
        let r = h.drag(hp, hp + vec2(0.0, -40.0 * s), Modifiers::NONE);
        h.apply(&r);
        let k = h.kf(SEC);
        let (o, i) = (k.spatial_out.unwrap(), k.spatial_in.unwrap());
        assert!((o[1] + 40.0).abs() < 0.5, "{o:?}");
        // Collinear, opposite, and the in handle keeps its length (40).
        let (li, lo) = (i[0].hypot(i[1]), o[0].hypot(o[1]));
        assert!((li - 40.0).abs() < 0.5, "{i:?}");
        assert!(
            (i[0] / li + o[0] / lo).abs() < 1e-3 && (i[1] / li + o[1] / lo).abs() < 1e-3,
            "collinear: {i:?} {o:?}"
        );
        // Cmd/Ctrl-drag of the in handle: only it moves, the key is no longer
        // continuous.
        let ip = h
            .drawn()
            .handles
            .iter()
            .find(|(_, o, _)| !*o)
            .map(|(_, _, p)| *p)
            .unwrap();
        let before = h.kf(SEC).spatial_out;
        let r = h.drag(ip, ip + vec2(0.0, 30.0 * s), cmd);
        h.apply(&r);
        let k = h.kf(SEC);
        assert_eq!(k.spatial_out, before);
        assert!(!k.flags.spatial_continuous);
        // Every drag was one undo step.
        h.stage.undo(&h.author).unwrap();
        h.stage.undo(&h.author).unwrap();
        h.stage.undo(&h.author).unwrap();
        assert!(h.kf(SEC).spatial_out.is_none_or(|t| t == [0.0; 3]));
    }

    #[test]
    fn the_motion_path_preference_limits_what_shows() {
        let mut h = H::new();
        let mut prefs = bae_prefs::Preferences::default();
        prefs.set(bae_prefs::keys::MOTION_PATH_MODE, "max_keyframes");
        prefs.set("appearance.motion_path_max_keyframes", 2);
        h.resources
            .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
        h.frame(Vec::new(), Modifiers::NONE);
        assert_eq!(h.drawn().keys.len(), 2);
        let mut prefs = bae_prefs::Preferences::default();
        prefs.set(bae_prefs::keys::MOTION_PATH_MODE, "no_motion_path");
        h.resources
            .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
        h.frame(Vec::new(), Modifiers::NONE);
        assert!(h.drawn().keys.is_empty() && h.drawn().dots.is_empty());
        // Not selected: no path.
        let mut prefs = bae_prefs::Preferences::default();
        prefs.set(bae_prefs::keys::MOTION_PATH_MODE, "all_keyframes");
        h.resources
            .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
        h.session().lock().selected_layers.clear();
        h.frame(Vec::new(), Modifiers::NONE);
        assert!(h.drawn().keys.is_empty());
    }
}
