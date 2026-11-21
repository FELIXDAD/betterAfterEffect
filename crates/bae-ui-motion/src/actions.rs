//! Motion workspace actions: the AE shortcuts that act on the session (current time,
//! selection, playback) and turn into `bae-motion` commands. Every motion panel
//! forwards its `on_action` here, so the shortcuts work whichever of them has focus.

use std::time::{Duration, Instant};

use bae_model::{LayerKind, PrimPath, Time};
use bae_motion::query::{self, CompInfo};
use bae_motion::{CopiedKey, KeyRef};
use bae_ui::{Binding, Invocation, KeymapBindings, PanelContext};
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
];

/// AE shortcuts of the motion workspace, added to the shell's `ae` keymap.
pub fn bindings() -> KeymapBindings {
    let b = Binding::new;
    let step = |chord: &str, frames: i64| b(chord, ids::STEP).params(json!({ "frames": frames }));
    let ease = |chord: &str, mode: &str| b(chord, ids::EASE).params(json!({ "mode": mode }));
    let reveal = |chord: &str, what: &str, add: bool| {
        b(chord, ids::REVEAL).params(json!({ "what": what, "add": add }))
    };
    let key = |chord: &str, attr: &str| b(chord, ids::TOGGLE_KEY).params(json!({ "attr": attr }));
    let layer = |chord: &str, kind: &str| b(chord, ids::NEW_LAYER).params(json!({ "kind": kind }));
    let order = |chord: &str, to: &str| b(chord, ids::ORDER).params(json!({ "to": to }));
    let mut v = vec![
        b("Space", ids::PLAY),
        b("Home", ids::GO_START),
        b("End", ids::GO_END),
        step("PageUp", -1),
        step("PageDown", 1),
        step("Cmd+ArrowLeft", -1),
        step("Cmd+ArrowRight", 1),
        step("Shift+PageUp", -10),
        step("Shift+PageDown", 10),
        b("J", ids::PREV_KEY),
        b("K", ids::NEXT_KEY),
        b("I", ids::GO_IN),
        b("O", ids::GO_OUT),
        b("B", ids::WORK_AREA_START),
        b("N", ids::WORK_AREA_END),
        b("Alt+OpenBracket", ids::TRIM_IN),
        b("Alt+CloseBracket", ids::TRIM_OUT),
        b("OpenBracket", ids::MOVE_IN),
        b("CloseBracket", ids::MOVE_OUT),
        b("Cmd+D", ids::DUPLICATE),
        b("Cmd+Shift+D", ids::SPLIT),
        b("Delete", ids::DELETE),
        b("Backspace", ids::DELETE),
        ease("F9", "easy_ease"),
        ease("Shift+F9", "ease_in"),
        ease("Cmd+Shift+F9", "ease_out"),
        b("Cmd+Alt+H", ids::TOGGLE_HOLD),
        b("Cmd+A", ids::SELECT_ALL),
        b("Cmd+Shift+A", ids::DESELECT_ALL),
        b("F2", ids::DESELECT_ALL),
        reveal("U", "animated", false),
        b("Cmd+N", ids::NEW_COMP),
        b("Cmd+Shift+X", ids::TRIM_COMP),
        b("Cmd+M", ids::ADD_TO_RENDER_QUEUE),
        layer("Cmd+Y", "solid"),
        layer("Cmd+Alt+Y", "adjustment"),
        layer("Cmd+Alt+Shift+Y", "null"),
        layer("Cmd+Alt+Shift+T", "text"),
        layer("Cmd+Alt+Shift+C", "camera"),
        layer("Cmd+Alt+Shift+L", "light"),
        order("Cmd+CloseBracket", "forward"),
        order("Cmd+OpenBracket", "backward"),
        order("Cmd+Shift+CloseBracket", "front"),
        order("Cmd+Shift+OpenBracket", "back"),
    ];
    for (k, attr) in [
        ("P", "transform:position"),
        ("S", "transform:scale"),
        ("R", "transform:rotation"),
        ("T", "transform:opacity"),
        ("A", "transform:anchor_point"),
    ] {
        v.push(reveal(k, attr, false));
        v.push(reveal(&format!("Shift+{k}"), attr, true));
        v.push(key(&format!("Alt+Shift+{k}"), attr));
    }
    v.push(b("Equals", "timeline.zoom_in").when("timeline"));
    v.push(b("Minus", "timeline.zoom_out").when("timeline"));
    v.push(b("Period", "viewer.zoom_in").when("composition"));
    v.push(b("Comma", "viewer.zoom_out").when("composition"));
    v.push(b("Shift+Slash", "viewer.fit").when("composition"));
    KeymapBindings::new(bae_ui::presets::AE_KEYMAP, v).for_workspace(crate::menus::WORKSPACE)
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
