//! Motion workspace actions: the AE shortcuts that act on the session (current time,
//! selection, playback) and turn into `bae-motion` commands. Every motion panel
//! forwards its `on_action` here, so the shortcuts work whichever of them has focus.

use std::time::{Duration, Instant};

use bae_model::{LayerKind, PrimPath, Time};
use bae_motion::query::{self, CompInfo};
use bae_motion::{CopiedKey, KeyRef};
use bae_ui::{Binding, CommandVariant, Invocation, KeymapBindings, PanelContext};
use serde_json::json;

use crate::common;
use crate::session::{KeyClipboard, Pending, Reveal, SharedSession};

pub mod ids {
    pub const PLAY: &str = "motion.play";
    pub const GO_START: &str = "motion.go_start";
    pub const GO_END: &str = "motion.go_end";
    /// Params `{"frames": n}`.
    pub const STEP: &str = "motion.step";
    pub const PREV_KEY: &str = "motion.prev_key";
    pub const NEXT_KEY: &str = "motion.next_key";
    pub const GO_IN: &str = "motion.go_in";
    pub const GO_OUT: &str = "motion.go_out";
    pub const WORK_AREA_START: &str = "motion.work_area_start";
    pub const WORK_AREA_END: &str = "motion.work_area_end";
    pub const TRIM_IN: &str = "motion.trim_in";
    pub const TRIM_OUT: &str = "motion.trim_out";
    pub const MOVE_IN: &str = "motion.move_in";
    pub const MOVE_OUT: &str = "motion.move_out";
    pub const DUPLICATE: &str = "motion.duplicate";
    pub const SPLIT: &str = "motion.split";
    pub const DELETE: &str = "motion.delete";
    /// Params `{"mode": "easy_ease" | "ease_in" | "ease_out"}`.
    pub const EASE: &str = "motion.ease";
    pub const TOGGLE_HOLD: &str = "motion.toggle_hold";
    pub const REVERSE_KEYS: &str = "motion.reverse_keys";
    pub const SELECT_ALL: &str = "motion.select_all";
    pub const DESELECT_ALL: &str = "motion.deselect_all";
    /// Params `{"what": "animated" | "<attr>", "add": bool}`.
    pub const REVEAL: &str = "motion.reveal";
    /// Params `{"attr": "<attr>"}`.
    pub const TOGGLE_KEY: &str = "motion.toggle_key";
    pub const NEW_COMP: &str = "motion.new_comp";
    /// Params `{"kind": "<layer kind>", "shape": "rect" | …}`.
    pub const NEW_LAYER: &str = "motion.new_layer";
    /// Params `{"to": "forward" | "backward" | "front" | "back"}`.
    pub const ORDER: &str = "motion.order";
    pub const TRIM_COMP: &str = "motion.trim_comp";
    /// Queue the active comp in the Render Queue (AE Cmd/Ctrl+M).
    pub const ADD_TO_RENDER_QUEUE: &str = "motion.add_to_render_queue";
    /// Shell clipboard actions (Cmd+C / Cmd+V outside text fields).
    pub const COPY: &str = "edit.copy";
    pub const PASTE: &str = "edit.paste";
}

/// `(id, English title)` of every motion action, for the shortcut editor.
pub const ACTIONS: &[(&str, &str)] = &[
    (ids::PLAY, "Play/Stop"),
    (ids::GO_START, "Go to Start"),
    (ids::GO_END, "Go to End"),
    (ids::STEP, "Next Frame"),
    (ids::PREV_KEY, "Go to Previous Keyframe"),
    (ids::NEXT_KEY, "Go to Next Keyframe"),
    (ids::GO_IN, "Go to Layer In Point"),
    (ids::GO_OUT, "Go to Layer Out Point"),
    (ids::WORK_AREA_START, "Set Work Area Start"),
    (ids::WORK_AREA_END, "Set Work Area End"),
    (ids::TRIM_IN, "Trim Layer In Point"),
    (ids::TRIM_OUT, "Trim Layer Out Point"),
    (ids::MOVE_IN, "Move In Point to Current Time"),
    (ids::MOVE_OUT, "Move Out Point to Current Time"),
    (ids::DUPLICATE, "Duplicate"),
    (ids::SPLIT, "Split Layer"),
    (ids::DELETE, "Delete"),
    (ids::EASE, "Easy Ease"),
    (ids::TOGGLE_HOLD, "Toggle Hold Keyframe"),
    (ids::REVERSE_KEYS, "Time-Reverse Keyframes"),
    (ids::SELECT_ALL, "Select All"),
    (ids::DESELECT_ALL, "Deselect All"),
    (ids::REVEAL, "Reveal Animated Properties"),
    (ids::TOGGLE_KEY, "Add or Remove Keyframe at Current Time"),
    (ids::NEW_COMP, "New Composition"),
    (ids::NEW_LAYER, "New"),
    (ids::ORDER, "Bring Layer Forward"),
    (ids::TRIM_COMP, "Trim Comp to Work Area"),
    (ids::ADD_TO_RENDER_QUEUE, "Add to Render Queue"),
    // Edit > Copy / Paste (Cmd+C / Cmd+V): selected keyframes.
    (ids::COPY, "Copy"),
    (ids::PASTE, "Paste"),
    // Panel-scoped (`bindings`): handled by the timeline and the viewer. Registered
    // so the Keyboard Shortcuts window shows a name, not the id (#136).
    ("timeline.zoom_in", "Zoom In Time"),
    ("timeline.zoom_out", "Zoom Out Time"),
    ("viewer.zoom_in", "Zoom In"),
    ("viewer.zoom_out", "Zoom Out"),
    ("viewer.fit", "Fit"),
];

/// The properties the P / S / R / T / A shortcuts act on, with their English names.
const TRANSFORM_PROPERTIES: [(&str, &str, &str); 5] = [
    ("P", "transform:position", "Position"),
    ("S", "transform:scale", "Scale"),
    ("R", "transform:rotation", "Rotation"),
    ("T", "transform:opacity", "Opacity"),
    ("A", "transform:anchor_point", "Anchor Point"),
];

/// Variants with fixed parameters: (command, parameters as JSON, English name). A
/// table rather than `json!` calls keeps the code small (parsed once at startup).
const VARIANTS: &[(&str, &str, &str)] = &[
    (ids::STEP, r#"{"frames":-1}"#, "Previous Frame"),
    (ids::STEP, r#"{"frames":1}"#, "Next Frame"),
    (ids::STEP, r#"{"frames":-10}"#, "Back 10 Frames"),
    (ids::STEP, r#"{"frames":10}"#, "Forward 10 Frames"),
    (ids::EASE, r#"{"mode":"easy_ease"}"#, "Easy Ease"),
    (ids::EASE, r#"{"mode":"ease_in"}"#, "Easy Ease In"),
    (ids::EASE, r#"{"mode":"ease_out"}"#, "Easy Ease Out"),
    ("key.ease", r#"{"mode":"easy_ease"}"#, "Easy Ease"),
    ("key.ease", r#"{"mode":"ease_in"}"#, "Easy Ease In"),
    ("key.ease", r#"{"mode":"ease_out"}"#, "Easy Ease Out"),
    (ids::ORDER, r#"{"to":"forward"}"#, "Bring Layer Forward"),
    (ids::ORDER, r#"{"to":"backward"}"#, "Send Layer Backward"),
    (ids::ORDER, r#"{"to":"front"}"#, "Bring Layer to Front"),
    (ids::ORDER, r#"{"to":"back"}"#, "Send Layer to Back"),
    (
        ids::REVEAL,
        r#"{"what":"animated","add":false}"#,
        "Reveal Animated Properties",
    ),
];

/// Layer kinds by name, for both the UI action and the model command it runs.
const LAYER_VARIANTS: &[(&str, &str)] = &[
    (r#"{"kind":"solid"}"#, "New Solid"),
    (r#"{"kind":"adjustment"}"#, "New Adjustment Layer"),
    (r#"{"kind":"null"}"#, "New Null Object"),
    (r#"{"kind":"text"}"#, "New Text Layer"),
    (r#"{"kind":"camera"}"#, "New Camera"),
    (r#"{"kind":"light"}"#, "New Light"),
    (r#"{"kind":"shape"}"#, "New Shape Layer"),
    (r#"{"kind":"shape","shape":"rect"}"#, "New Rectangle"),
    (r#"{"kind":"shape","shape":"ellipse"}"#, "New Ellipse"),
];

/// Names of the variants of motion actions and of the model commands they run, so
/// the Keyboard Shortcuts window, Edit > Undo and History say which one (#140):
/// "Back 10 Frames", not four "Next Frame" rows; "Undo New Solid", not "Undo New
/// Layer". `(registry id, variant)`.
pub fn variants() -> Vec<(String, CommandVariant)> {
    let mut v = Vec::new();
    let mut add = |command: &str, params: serde_json::Value, title: String| {
        let id = format!("{command}/{params}");
        v.push((id, CommandVariant::new(command, params, title)));
    };
    let parse = |json: &str| serde_json::from_str(json).expect("variant parameters are JSON");
    for (command, params, title) in VARIANTS {
        add(command, parse(params), (*title).to_owned());
    }
    for (params, title) in LAYER_VARIANTS {
        add(ids::NEW_LAYER, parse(params), (*title).to_owned());
        add("layer.create", parse(params), (*title).to_owned());
    }
    for (_, attr, name) in TRANSFORM_PROPERTIES {
        for add_to_shown in [false, true] {
            let title = if add_to_shown {
                format!("Add {name} to Shown Properties")
            } else {
                format!("Show {name}")
            };
            add(
                ids::REVEAL,
                json!({ "what": attr, "add": add_to_shown }),
                title,
            );
        }
        add(
            ids::TOGGLE_KEY,
            json!({ "attr": attr }),
            format!("{name}: Add or Remove Keyframe"),
        );
    }
    v
}

/// AE's default shortcuts of the commands that run (catalog status done or
/// partial), added to the shell's `ae` keymap: every chord of AE's shortcut file
/// for this platform, scoped like AE's context, except the chords the shell's own
/// `ae` keymap already binds. Commands other modules handle are bound when the
/// app starts ([`late_bindings`]).
pub fn bindings() -> KeymapBindings {
    let v = without_base(crate::catalog::ae_bindings(crate::catalog::keys(), &|k| {
        k.status.works()
    }));
    KeymapBindings::new(bae_ui::presets::AE_KEYMAP, v).for_workspace(crate::menus::WORKSPACE)
}

/// AE's default shortcuts of the commands another module handles (`handled`,
/// `bae_ui::menu::handled_actions`) that are not done in the catalog yet, minus
/// the chords and commands already bound (`taken`: the other keymap
/// extensions). An agent that implements an AE command gets AE's shortcut for
/// it without touching the catalog.
pub fn late_bindings(
    handled: &std::collections::HashSet<String>,
    taken: &[Binding],
) -> Vec<Binding> {
    let live = |k: &crate::catalog::Key| {
        !k.status.works() && crate::catalog::live_by_handler(k.action, k.params, handled)
    };
    without_base(crate::catalog::ae_bindings(crate::catalog::keys(), &live))
        .into_iter()
        .filter(|b| {
            !taken.iter().any(|t| {
                (t.chord == b.chord && t.when == b.when)
                    || (t.action == b.action && t.params == b.params)
            })
        })
        .collect()
}

/// Drops the bindings whose chord the shell's `ae` keymap binds already (its
/// File, Edit, Window shortcuts).
fn without_base(v: Vec<Binding>) -> Vec<Binding> {
    let base = bae_ui::presets::ae_keymap();
    v.into_iter()
        .filter(|b| {
            !base
                .bindings
                .iter()
                .any(|x| x.chord == b.chord && x.when == b.when)
        })
        .collect()
}

/// Selected layers, or the layers of the selected keyframes.
/// The layers an action applies to: the selected ones (or those of the
/// selected keys), never locked ones (AE: menu commands do not act on them).
fn target_layers(stage: &bae_model::Stage, s: &crate::session::Session) -> Vec<PrimPath> {
    let mut out: Vec<PrimPath> = if !s.selected_layers.is_empty() {
        s.selected_layers.clone()
    } else {
        let mut out: Vec<PrimPath> = Vec::new();
        for k in &s.selected_keys {
            if let Some(l) = query::owning_layer(stage, &k.path)
                && !out.contains(&l)
            {
                out.push(l);
            }
        }
        out
    };
    out.retain(|l| !query::is_locked(stage, l));
    out
}

/// Comp times of every keyframe of `layer` and the prims below it.
pub(crate) fn layer_key_times(stage: &bae_model::Stage, layer: &PrimPath) -> Vec<Time> {
    let mut out = Vec::new();
    let mut stack = vec![layer.clone()];
    while let Some(p) = stack.pop() {
        if let Some(v) = stage.prim(&p) {
            for name in v.attr_names() {
                if let Some(spec) = v.attr(name) {
                    for t in spec.keyframes.keys() {
                        out.push(query::to_comp_time(stage, &p, *t));
                    }
                }
            }
        }
        stack.extend(stage.children(&p));
    }
    out
}

fn jump_to_key(
    stage: &bae_model::Stage,
    s: &mut crate::session::Session,
    info: &CompInfo,
    next: bool,
) {
    let layers = {
        let t = target_layers(stage, s);
        if t.is_empty() {
            query::comp_layers(stage, &info.path)
        } else {
            t
        }
    };
    let now = s.time(&info.path);
    let mut times: Vec<Time> = Vec::new();
    for l in &layers {
        times.extend(layer_key_times(stage, l));
        let timing = query::layer_timing(stage, l);
        times.push(timing.in_point);
        times.push(timing.out_point - info.frame_duration());
    }
    let snapped: Vec<Time> = times.into_iter().map(|t| info.frame_rate.snap(t)).collect();
    let pick = if next {
        snapped.into_iter().filter(|t| *t > now).min()
    } else {
        snapped.into_iter().filter(|t| *t < now).max()
    };
    if let Some(t) = pick {
        s.set_time(info, t);
    }
}

/// Handle a motion action. Returns `false` for ids it does not know.
pub(crate) fn handle(inv: &Invocation, cx: &mut PanelContext<'_>, session: &SharedSession) -> bool {
    let stage = cx.stage;
    let id = inv.id.as_str();
    if !id.starts_with("motion.") && id != ids::COPY && id != ids::PASTE {
        return false;
    }
    match id {
        ids::NEW_COMP => {
            common::new_comp(cx, session, None);
            return true;
        }
        ids::NEW_LAYER => {
            let kind = inv
                .params
                .get("kind")
                .cloned()
                .and_then(|k| serde_json::from_value::<LayerKind>(k).ok())
                .unwrap_or(LayerKind::Solid);
            let mut extra = json!({});
            if let Some(shape) = inv.params.get("shape") {
                extra["shape"] = shape.clone();
            }
            if kind == LayerKind::Text {
                extra["text"] = json!(bae_ui::tr("Text"));
            }
            common::new_layer(cx, session, kind, extra);
            return true;
        }
        _ => {}
    }
    let Some(info) = common::active_comp(stage, session) else {
        return true;
    };
    let mut s = session.lock();
    let now = s.time(&info.path);
    let frame = info.frame_duration();
    let layers = target_layers(stage, &s);
    let keys = s.selected_keys.clone();
    match id {
        ids::PLAY => {
            if s.is_playing() {
                s.stop();
            } else {
                s.play(&info);
            }
        }
        ids::GO_START => s.set_time(&info, Time::ZERO),
        ids::GO_END => s.set_time(&info, info.duration),
        ids::STEP => {
            let n = inv
                .params
                .get("frames")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            s.stop();
            s.set_time(&info, now + Time(frame.0 * n));
        }
        ids::PREV_KEY | ids::NEXT_KEY => jump_to_key(stage, &mut s, &info, id == ids::NEXT_KEY),
        ids::GO_IN | ids::GO_OUT => {
            if let Some(l) = layers.first() {
                let t = query::layer_timing(stage, l);
                let to = if id == ids::GO_IN {
                    t.in_point
                } else {
                    t.out_point - frame
                };
                s.set_time(&info, to);
            }
        }
        ids::WORK_AREA_START => {
            cx.invoke_with(
                "comp.set_work_area",
                json!({"comp": info.path, "start": now}),
            );
        }
        ids::WORK_AREA_END => {
            cx.invoke_with(
                "comp.set_work_area",
                json!({"comp": info.path, "end": now + frame}),
            );
        }
        ids::TRIM_COMP => {
            cx.invoke_with("comp.trim_to_work_area", json!({"comp": info.path}));
        }
        ids::ADD_TO_RENDER_QUEUE => {
            crate::render_queue::shared(cx.resources)
                .lock()
                .add(&info, &crate::render_queue::default_output_dir());
            cx.open_panel("render_queue");
        }
        ids::TRIM_IN if !layers.is_empty() => {
            cx.invoke_with("layer.trim", json!({"layers": layers, "in_point": now}));
        }
        ids::TRIM_OUT if !layers.is_empty() => {
            cx.invoke_with(
                "layer.trim",
                json!({"layers": layers, "out_point": now + frame}),
            );
        }
        ids::MOVE_IN | ids::MOVE_OUT => {
            for l in &layers {
                let t = query::layer_timing(stage, l);
                let delta = if id == ids::MOVE_IN {
                    now - t.in_point
                } else {
                    now + frame - t.out_point
                };
                if delta != Time::ZERO {
                    cx.invoke_with("layer.shift_time", json!({"layers": [l], "delta": delta}));
                }
            }
        }
        // AE selects the new layers, so the next edit (or the next split
        // further on) applies to them.
        ids::DUPLICATE if !layers.is_empty() => {
            cx.invoke_with("layer.duplicate", json!({"layers": layers}));
            s.expect(Pending::NewLayers {
                comp: info.path.clone(),
                before: query::comp_layers(stage, &info.path),
            });
        }
        ids::SPLIT if !layers.is_empty() => {
            cx.invoke_with("layer.split", json!({"layers": layers, "time": now}));
            s.expect(Pending::NewLayers {
                comp: info.path.clone(),
                before: query::comp_layers(stage, &info.path),
            });
        }
        ids::DELETE => {
            if !keys.is_empty() {
                cx.invoke_with("key.delete", json!({"keys": keys}));
                s.selected_keys.clear();
            } else if !s.selected_layers.is_empty() {
                cx.invoke_with(
                    "layer.delete",
                    json!({"layers": s.selected_layers, "time": now}),
                );
                s.selected_layers.clear();
            }
        }
        ids::EASE if !keys.is_empty() => {
            let mode = inv
                .params
                .get("mode")
                .cloned()
                .unwrap_or(json!("easy_ease"));
            cx.invoke_with("key.ease", json!({"keys": keys, "mode": mode}));
        }
        ids::TOGGLE_HOLD if !keys.is_empty() => {
            let all_hold = keys.iter().all(|k| {
                query::attr_spec(stage, &k.path, &k.attr)
                    .and_then(|a| a.keyframes.get(&k.time))
                    .is_some_and(|kf| kf.interp_out == bae_model::Interpolation::Hold)
            });
            let to = if all_hold { "linear" } else { "hold" };
            cx.invoke_with(
                "key.set_interpolation",
                json!({"keys": keys, "interp_out": to}),
            );
        }
        ids::REVERSE_KEYS if keys.len() > 1 => {
            cx.invoke_with("key.reverse", json!({"keys": keys}));
        }
        ids::SELECT_ALL => {
            // AE: Select All skips locked layers.
            s.selected_layers = query::comp_layers(stage, &info.path)
                .into_iter()
                .filter(|l| !query::is_locked(stage, l))
                .collect();
        }
        ids::DESELECT_ALL => {
            s.selected_layers.clear();
            s.selected_keys.clear();
        }
        ids::REVEAL => {
            let what = inv
                .params
                .get("what")
                .and_then(|v| v.as_str())
                .unwrap_or("animated");
            let add = inv.params.get("add").and_then(|v| v.as_bool()) == Some(true);
            let targets = if layers.is_empty() {
                query::comp_layers(stage, &info.path)
            } else {
                layers.clone()
            };
            let reveal = if what == "animated" {
                let double = s
                    .last_u
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(450));
                s.last_u = Some(Instant::now());
                if double {
                    Some(Reveal::Modified)
                } else {
                    Some(Reveal::Animated)
                }
            } else {
                None
            };
            for l in targets {
                let current = s.reveal.get(&l).cloned();
                let next = match (&reveal, current) {
                    (Some(Reveal::Animated), Some(Reveal::Animated)) => None,
                    (Some(r), _) => Some(r.clone()),
                    (None, Some(Reveal::Attrs(mut v))) if add => {
                        if let Some(i) = v.iter().position(|a| a == what) {
                            v.remove(i);
                        } else {
                            v.push(what.to_owned());
                        }
                        (!v.is_empty()).then_some(Reveal::Attrs(v))
                    }
                    (None, Some(Reveal::Attrs(v))) if v == [what.to_owned()] => None,
                    (None, _) => Some(Reveal::Attrs(vec![what.to_owned()])),
                };
                match next {
                    Some(r) => {
                        s.reveal.insert(l, r);
                    }
                    None => {
                        s.reveal.remove(&l);
                    }
                }
            }
        }
        ids::TOGGLE_KEY => {
            let Some(attr) = inv.params.get("attr").and_then(|v| v.as_str()) else {
                return true;
            };
            for l in &layers {
                let kt = query::to_key_time(stage, l, now);
                let spec = query::attr_spec(stage, l, attr);
                match spec {
                    Some(a) if a.keyframes.contains_key(&kt) => {
                        cx.invoke_with(
                            "key.delete",
                            json!({"keys": [KeyRef::new(l.clone(), attr, kt)]}),
                        );
                    }
                    Some(a) if a.is_animated() => {
                        if let Some(v) = query::sample_attr(stage, l, attr, kt) {
                            cx.invoke_with(
                                "attr.set_at",
                                json!({"path": l, "attr": attr, "time": kt, "value": v}),
                            );
                        }
                    }
                    _ => cx.invoke_with(
                        "attr.set_animated",
                        json!({"path": l, "attr": attr, "animated": true, "time": kt}),
                    ),
                }
                let mut r = match s.reveal.get(l) {
                    Some(Reveal::Attrs(v)) => v.clone(),
                    _ => Vec::new(),
                };
                if !r.iter().any(|a| a == attr) {
                    r.push(attr.to_owned());
                }
                s.reveal.insert(l.clone(), Reveal::Attrs(r));
            }
        }
        ids::ORDER if !s.selected_layers.is_empty() => {
            let all = query::comp_layers(stage, &info.path);
            let first = s
                .selected_layers
                .iter()
                .filter_map(|l| all.iter().position(|x| x == l))
                .min()
                .unwrap_or(0);
            let index = match inv.params.get("to").and_then(|v| v.as_str()) {
                Some("front") => 0,
                Some("back") => all.len(),
                Some("backward") => first + 1,
                _ => first.saturating_sub(1),
            };
            cx.invoke_with(
                "layer.reorder",
                json!({"layers": s.selected_layers, "index": index}),
            );
        }
        ids::COPY => {
            let Some(first) = keys.first() else {
                return false;
            };
            let same: Vec<&KeyRef> = keys
                .iter()
                .filter(|k| k.path == first.path && k.attr == first.attr)
                .collect();
            let t0 = same.iter().map(|k| k.time).min().unwrap_or(first.time);
            let spec = query::attr_spec(stage, &first.path, &first.attr);
            let copied: Vec<CopiedKey> = same
                .iter()
                .filter_map(|k| {
                    Some(CopiedKey {
                        offset: k.time - t0,
                        keyframe: spec?.keyframes.get(&k.time)?.clone(),
                    })
                })
                .collect();
            s.clipboard = Some(KeyClipboard {
                attr: first.attr.clone(),
                keys: copied,
            });
        }
        ids::PASTE => {
            let Some(clip) = s.clipboard.clone() else {
                return false;
            };
            for l in &layers {
                let kt = query::to_key_time(stage, l, now);
                cx.invoke_with(
                    "key.paste",
                    json!({"path": l, "attr": clip.attr, "time": kt, "keys": clip.keys}),
                );
            }
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_parse_and_do_not_conflict_with_the_shell() {
        let ext = bindings();
        let base = bae_ui::presets::ae_keymap();
        let mut all = base.bindings.clone();
        all.extend(ext.bindings.iter().cloned());
        let km = bae_ui::Keymap::new("test", all);
        assert!(km.conflicts().is_empty(), "{:?}", km.conflicts());
    }
}
