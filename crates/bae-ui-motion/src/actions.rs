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
    /// Params `{"what": "animated" | "animation" | "modified" | "<attr>", "add": bool}`.
    pub const REVEAL: &str = "motion.reveal";
    /// Params `{"attr": "<attr>"}`.
    pub const TOGGLE_KEY: &str = "motion.toggle_key";
    /// Composition > New Composition (Cmd/Ctrl+N): opens Composition Settings.
    pub const NEW_COMP: &str = "motion.new_comp";
    /// Composition > Composition Settings (Cmd/Ctrl+K): the active comp, or the
    /// comp selected in the focused Project panel.
    pub const COMP_SETTINGS: &str = "motion.comp.composition_settings";
    /// Params `{"kind": "<layer kind>", "shape": "rect" | …}`. A solid opens
    /// Solid Settings first.
    pub const NEW_LAYER: &str = "motion.new_layer";
    /// Layer > Layer Settings (Cmd/Ctrl+Shift+Y): Solid, Camera or Light
    /// Settings of the selected layer, or Solid Settings of the solid item
    /// selected in the focused Project panel.
    pub const LAYER_SETTINGS: &str = "motion.layer.layer_settings";
    /// Layer > Pre-compose (Cmd/Ctrl+Shift+C): the selected layers.
    pub const PRE_COMPOSE: &str = "motion.layer.pre_compose";
    /// File > Import > File (Cmd/Ctrl+I).
    pub const IMPORT_FILE: &str = "motion.file.import_file";
    /// File > Import > Multiple Files (Cmd/Ctrl+Alt+I).
    pub const IMPORT_MULTIPLE: &str = "motion.file.import_multiple_files";
    /// Params `{"to": "forward" | "backward" | "front" | "back"}`.
    pub const ORDER: &str = "motion.order";
    pub const TRIM_COMP: &str = "motion.trim_comp";
    /// Queue the active comp in the Render Queue (AE Cmd/Ctrl+M).
    pub const ADD_TO_RENDER_QUEUE: &str = "motion.add_to_render_queue";
    /// Toggle the Timeline between layer bars and the Graph Editor (AE Shift+F3).
    pub const GRAPH_EDITOR: &str = "motion.timeline.graph_editor";
    /// Animation > Add Expression (AE Alt+Shift+=): toggles the selected
    /// properties' expressions.
    pub const ADD_EXPRESSION: &str = "motion.anim.add_expression";
    /// Layer > Switches > Enable / Disable Expressions: every expression of the
    /// selected layers.
    pub const ENABLE_EXPRESSIONS: &str = "motion.layer.enable_expressions";
    pub const DISABLE_EXPRESSIONS: &str = "motion.layer.disable_expressions";
    /// Layer > Reveal > Reveal Expression Errors: the properties whose expressions
    /// fail, revealed in the Timeline.
    pub const REVEAL_EXPRESSION_ERRORS: &str = "motion.layer.reveal_expression_errors";
    /// Animation > Keyframe Assistant > Convert Expression to Keyframes.
    pub const CONVERT_EXPRESSION: &str = "motion.anim.convert_expression_to_keyframes";
    /// Animation > Keyframe Interpolation… (AE Cmd/Ctrl+Alt+K).
    pub const KEY_INTERPOLATION: &str = "motion.anim.keyframe_interpolation";
    /// Animation > Keyframe Velocity… (AE Cmd/Ctrl+Shift+K).
    pub const KEY_VELOCITY: &str = "motion.anim.keyframe_velocity";
    /// Edit > Select Keyframe Label Group ▸ (params `{"scope": "selected_layers" |
    /// "all_layers" | "visible_selected_layers" | "visible_all_layers"}`).
    pub const SELECT_KEY_LABEL_GROUP: &str = "motion.edit.select_keyframe_label_group";
    /// Layer > Markers > Add Marker (AE numpad *, Mac Control+8): a layer marker at
    /// the current time on each selected layer, or a comp marker when none is.
    pub const ADD_MARKER: &str = "motion.layer.add_marker";
    /// Animation > Animate Text > … (params `{"property": "position"}`): a new
    /// animator on each selected text layer.
    pub const ANIMATE_TEXT: &str = "motion.anim.animate_text";
    /// Animation > Add Text Selector > Range: to the last animator.
    pub const ADD_TEXT_SELECTOR: &str = "motion.anim.add_text_selector";
    pub const REMOVE_ALL_TEXT_ANIMATORS: &str = "motion.anim.remove_all_text_animators";
    /// Numbered comp marker N at the current time (AE Shift+0–9 on the main
    /// keyboard; params `{"number": n}`): the comp marker whose comment is "N",
    /// moved here when it exists, else added.
    pub const SET_COMP_MARKER: &str = "motion.timeline.set_comp_marker";
    /// Go to numbered comp marker N (AE 0–9 on the main keyboard).
    pub const GO_TO_COMP_MARKER: &str = "motion.timeline.go_to_comp_marker";
    /// Add a marker through the Marker dialog (AE Alt+numpad *, Mac Option+Control+8).
    pub const ADD_MARKER_DIALOG: &str = "motion.timeline.add_marker_with_dialog";
    /// Layer > Markers > Delete All Markers (of the selected layers).
    pub const DELETE_ALL_MARKERS: &str = "motion.layer.delete_all_markers";
    /// Shell clipboard actions (Cmd+C / Cmd+V outside text fields).
    pub const COPY: &str = "edit.copy";
    pub const PASTE: &str = "edit.paste";
    /// Edit > Paste Reversed Keyframes (AE Cmd/Ctrl+Shift+V).
    pub const PASTE_REVERSED: &str = "motion.edit.paste_reversed_keyframes";
    /// Edit > Cut: Copy, then Clear.
    pub const CUT: &str = "edit.cut";
    /// Layer ▸ Switches ▸ Shy, Lock … and Layer ▸ 3D Layer: params `{"switch":
    /// "<attr>"}` (see [`super::SWITCHES`]); toggles the selected layers.
    pub const LAYER_SWITCH: &str = "motion.layer.switch";
    /// Layer ▸ Track Matte ▸ … (params `{"matte": "alpha", "source": "above"}`).
    pub const TRACK_MATTE: &str = "motion.layer.track_matte";
    /// F4 / Shift+F4 (the Timeline handles them).
    pub const TOGGLE_SWITCHES_MODES: &str = "motion.timeline.toggle_switches_modes";
    pub const TOGGLE_PARENT_COLUMN: &str = "motion.timeline.toggle_parent_column";
    /// Layer ▸ Blending Mode ▸ … (params `{"mode": "multiply"}`).
    pub const BLENDING_MODE: &str = "motion.layer.blending_mode";
    /// Layer ▸ Next / Previous Blending Mode (AE Shift+= / Shift+-; `{"step": ±1}`).
    pub const BLENDING_MODE_STEP: &str = "motion.layer.blending_mode_step";
    pub const UNLOCK_ALL_LAYERS: &str = "motion.layer.unlock_all_layers";
    pub const SHOW_ALL_VIDEO: &str = "motion.layer.show_all_video";
    pub const HIDE_OTHER_VIDEO: &str = "motion.layer.hide_other_video";
    /// Layer ▸ Quality: params `{"quality": "best" | "draft"}`.
    pub const QUALITY: &str = "motion.layer.quality";
    /// Edit ▸ Label ▸ colour: params `{"label": 0..=16}`.
    pub const LABEL: &str = "motion.edit.label";
    pub const SELECT_LABEL_GROUP: &str = "motion.edit.select_label_group";
    /// Layer ▸ Transform ▸ Flip Horizontal / Flip Vertical (Scale X / Y negated).
    pub const FLIP_HORIZONTAL: &str = "motion.layer.flip_horizontal";
    pub const FLIP_VERTICAL: &str = "motion.layer.flip_vertical";
    /// Layer ▸ Transform ▸ Center In View (AE Ctrl+Home): the anchor point to the
    /// middle of the comp.
    pub const CENTER_IN_VIEW: &str = "motion.layer.center_in_view";
    /// Layer ▸ Transform ▸ Fit to Comp (Ctrl+Alt+F), Fit to Comp Width
    /// (Ctrl+Alt+Shift+H), Fit to Comp Height (Ctrl+Alt+Shift+G).
    pub const FIT_TO_COMP: &str = "motion.layer.fit_to_comp";
    pub const FIT_TO_COMP_WIDTH: &str = "motion.layer.fit_to_comp_width";
    pub const FIT_TO_COMP_HEIGHT: &str = "motion.layer.fit_to_comp_height";
}

/// The layer switches the Layer menu toggles (`layer.set_attr` attributes) with
/// their English names (AE's menu titles).
pub const SWITCHES: &[(&str, &str)] = &[
    ("shy", "Shy"),
    ("locked", "Lock"),
    ("audio_enabled", "Audio"),
    ("enabled", "Video"),
    ("effects_enabled", "Effect"),
    ("solo", "Solo"),
    ("collapse_transformation", "Collapse"),
    ("motion_blur", "Motion Blur"),
    ("adjustment_layer", "Adjustment Layer"),
    ("three_d", "3D Layer"),
    ("preserve_transparency", "Preserve Transparency"),
];

/// Whether a switch is on for a layer.
pub(crate) fn switch_on(stage: &bae_model::Stage, layer: &PrimPath, attr: &str) -> bool {
    stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, attr))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// A layer's quality (`best`, `draft`, `wireframe`).
pub(crate) fn quality_of(stage: &bae_model::Stage, layer: &PrimPath) -> String {
    stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "quality"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "best".to_owned())
}

/// A layer's label (0 = none, 1–16).
pub(crate) fn label_of(stage: &bae_model::Stage, layer: &PrimPath) -> i64 {
    stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "label"))
        .and_then(|v| v.as_f64())
        .map_or(0, |v| v as i64)
}

/// `(id, English title)` of every motion action, for the shortcut editor.
pub const ACTIONS: &[(&str, &str)] = &[
    (ids::ADD_EXPRESSION, "Add Expression"),
    (ids::ENABLE_EXPRESSIONS, "Enable Expressions"),
    (ids::DISABLE_EXPRESSIONS, "Disable Expressions"),
    (ids::REVEAL_EXPRESSION_ERRORS, "Reveal Expression Errors"),
    (ids::CONVERT_EXPRESSION, "Convert Expression to Keyframes"),
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
    (ids::COMP_SETTINGS, "Composition Settings"),
    (ids::NEW_LAYER, "New"),
    (ids::LAYER_SETTINGS, "Layer Settings"),
    (ids::PRE_COMPOSE, "Pre-compose"),
    (ids::IMPORT_FILE, "Import File"),
    (ids::IMPORT_MULTIPLE, "Import Multiple Files"),
    (ids::ORDER, "Bring Layer Forward"),
    (ids::TRIM_COMP, "Trim Comp to Work Area"),
    (ids::ADD_TO_RENDER_QUEUE, "Add to Render Queue"),
    // Edit > Copy / Paste (Cmd+C / Cmd+V): selected keyframes.
    (ids::COPY, "Copy"),
    (ids::CUT, "Cut"),
    (ids::PASTE, "Paste"),
    (ids::PASTE_REVERSED, "Paste Reversed Keyframes"),
    (ids::LAYER_SWITCH, "Layer Switch"),
    (ids::TRACK_MATTE, "Track Matte"),
    (
        ids::TOGGLE_SWITCHES_MODES,
        "Toggle Switches / Modes Columns",
    ),
    (ids::TOGGLE_PARENT_COLUMN, "Show / Hide Parent Column"),
    (ids::BLENDING_MODE, "Blending Mode"),
    (ids::BLENDING_MODE_STEP, "Next Blending Mode"),
    (ids::UNLOCK_ALL_LAYERS, "Unlock All Layers"),
    (ids::SHOW_ALL_VIDEO, "Show All Video"),
    (ids::HIDE_OTHER_VIDEO, "Hide Other Video"),
    (ids::QUALITY, "Quality"),
    (ids::LABEL, "Label"),
    (ids::FLIP_HORIZONTAL, "Flip Horizontal"),
    (ids::FLIP_VERTICAL, "Flip Vertical"),
    (ids::CENTER_IN_VIEW, "Center In View"),
    (ids::FIT_TO_COMP, "Fit to Comp"),
    (ids::FIT_TO_COMP_WIDTH, "Fit to Comp Width"),
    (ids::FIT_TO_COMP_HEIGHT, "Fit to Comp Height"),
    (ids::SELECT_LABEL_GROUP, "Select Label Group"),
    (ids::GRAPH_EDITOR, "Graph Editor"),
    (ids::KEY_INTERPOLATION, "Keyframe Interpolation"),
    (ids::KEY_VELOCITY, "Keyframe Velocity"),
    (ids::SELECT_KEY_LABEL_GROUP, "Select Keyframe Label Group"),
    (ids::ADD_MARKER, "Add Marker"),
    (ids::ANIMATE_TEXT, "Animate Text"),
    (ids::ADD_TEXT_SELECTOR, "Add Text Selector"),
    (ids::REMOVE_ALL_TEXT_ANIMATORS, "Remove All Text Animators"),
    (ids::SET_COMP_MARKER, "Set Composition Marker"),
    (ids::GO_TO_COMP_MARKER, "Go to Composition Marker"),
    (ids::ADD_MARKER_DIALOG, "Add Marker with Dialog"),
    (ids::DELETE_ALL_MARKERS, "Delete All Markers"),
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
    (
        ids::BLENDING_MODE_STEP,
        r#"{"step":1}"#,
        "Next Blending Mode",
    ),
    (
        ids::BLENDING_MODE_STEP,
        r#"{"step":-1}"#,
        "Previous Blending Mode",
    ),
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
    // Numbered comp markers (Shift+0–9, 0–9): one row per number.
    for n in 0..10 {
        let params = json!({ "number": n });
        add(
            ids::SET_COMP_MARKER,
            params.clone(),
            format!("Set Composition Marker {n}"),
        );
        add(
            ids::GO_TO_COMP_MARKER,
            params,
            format!("Go to Composition Marker {n}"),
        );
    }
    for (params, title) in LAYER_VARIANTS {
        add(ids::NEW_LAYER, parse(params), (*title).to_owned());
        add("layer.create", parse(params), (*title).to_owned());
    }
    for (attr, name) in SWITCHES {
        add(
            ids::LAYER_SWITCH,
            json!({ "switch": attr }),
            (*name).to_owned(),
        );
        add(
            "layer.set_attr",
            json!({ "attr": attr }),
            (*name).to_owned(),
        );
    }
    for (quality, name) in [
        ("best", "Best Quality"),
        ("draft", "Draft Quality"),
        ("wireframe", "Wireframe Quality"),
    ] {
        add(ids::QUALITY, json!({ "quality": quality }), name.to_owned());
    }
    add(
        "layer.set_attr",
        json!({ "attr": "quality" }),
        "Quality".to_owned(),
    );
    add(
        "layer.set_attr",
        json!({ "attr": "label" }),
        "Label".to_owned(),
    );
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

/// The comp Composition Settings edits: the one selected in the Project panel
/// when that panel has focus (AE), otherwise the active comp.
pub(crate) fn settings_target(cx: &PanelContext<'_>, session: &SharedSession) -> Option<PrimPath> {
    let s = session.lock();
    if cx.panel.kind() == "project"
        && let [item] = s.selected_items.as_slice()
        && query::comp_info(cx.stage, item).is_some()
    {
        return Some(item.clone());
    }
    s.active_comp
        .clone()
        .filter(|c| query::comp_info(cx.stage, c).is_some())
}

/// What Layer Settings edits: the solid item selected in the Project panel
/// when that panel has focus, otherwise the one selected layer when it has
/// settings (solid and adjustment layers, cameras, lights).
pub(crate) fn layer_settings_target(
    stage: &bae_model::Stage,
    session: &SharedSession,
    project_focused: bool,
) -> Option<PrimPath> {
    let s = session.lock();
    if project_focused
        && let [item] = s.selected_items.as_slice()
        && bae_motion::solid::solid_info(stage, item).is_some()
    {
        return Some(item.clone());
    }
    match s.selected_layers.as_slice() {
        [layer]
            if bae_motion::solid::layer_solid(stage, layer).is_some()
                || matches!(
                    query::layer_kind(stage, layer).as_deref(),
                    Some("camera" | "light")
                ) =>
        {
            Some(layer.clone())
        }
        _ => None,
    }
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

/// Numbered comp marker `n` of `comp` (the comp marker whose comment is the number,
/// as AE names the markers Shift+0–9 set) and its time.
pub(crate) fn numbered_comp_marker(
    stage: &bae_model::Stage,
    comp: &PrimPath,
    n: i64,
) -> Option<(PrimPath, Time)> {
    let label = n.to_string();
    bae_motion::markers::markers_of(stage, comp)
        .into_iter()
        .find(|(m, _)| {
            stage
                .prim(m)
                .and_then(|v| query::static_value(&v, "comment"))
                .and_then(|v| v.as_str().map(|c| c.trim() == label))
                .unwrap_or(false)
        })
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

/// Layer ▸ Transform commands on `layers` at comp time `now`, as `attr.set_many`
/// edits (a keyframe where the property is animated, like AE).
fn transform_edits(
    stage: &bae_model::Stage,
    info: &CompInfo,
    now: Time,
    layers: &[PrimPath],
    id: &str,
) -> Vec<serde_json::Value> {
    use bae_anim::Transform;
    use bae_anim::glam::{DMat4, DVec3};
    use bae_model::Value;
    let Ok(comp) = bae_anim::Evaluator::new().evaluate_comp(stage, &info.path, now) else {
        return Vec::new();
    };
    let center = DVec3::new(f64::from(info.width), f64::from(info.height), 0.0) / 2.0;
    // Move Position so that `local` (layer space) lands on the comp's center.
    let land = |t: &mut Transform, parent: DMat4, local: DVec3, three_d: bool| {
        let d = center - (parent * t.matrix(three_d)).transform_point3(local);
        t.position += parent
            .inverse()
            .transform_vector3(DVec3::new(d.x, d.y, 0.0));
    };
    let mut out = Vec::new();
    for l in comp.layers.iter().filter(|l| layers.contains(&l.path)) {
        let parent = l
            .parent
            .map_or(DMat4::IDENTITY, |i| comp.layers[i].world_matrix);
        let three_d = l.switches.three_d;
        let mut t = l.transform;
        match id {
            ids::FLIP_HORIZONTAL => t.scale.x = -t.scale.x,
            ids::FLIP_VERTICAL => t.scale.y = -t.scale.y,
            ids::CENTER_IN_VIEW => {
                let anchor = t.anchor_point;
                land(&mut t, parent, anchor, three_d);
            }
            _ => {
                let Some((a, b)) = crate::render::layer_bounds(l) else {
                    continue;
                };
                let size = b - a;
                if size.x <= 0.0 || size.y <= 0.0 {
                    continue;
                }
                let w = f64::from(info.width) / size.x * 100.0;
                let h = f64::from(info.height) / size.y * 100.0;
                let (sx, sy) = match id {
                    ids::FIT_TO_COMP => (w, h),
                    ids::FIT_TO_COMP_WIDTH => (w, w),
                    _ => (h, h),
                };
                t.scale.x = sx.copysign(t.scale.x);
                t.scale.y = sy.copysign(t.scale.y);
                let c = (a + b) / 2.0;
                land(&mut t, parent, DVec3::new(c.x, c.y, 0.0), three_d);
            }
        }
        let time = query::to_key_time(stage, &l.path, now);
        let mut push = |attr: &str, v: Value| {
            out.push(json!({"path": l.path, "attr": attr, "time": time, "value": v}));
        };
        if t.scale != l.transform.scale {
            let v = t.scale;
            push("transform:scale", Value::Vec3([v.x, v.y, v.z]));
        }
        if t.position != l.transform.position {
            let p = t.position;
            let separate = stage
                .prim(&l.path)
                .and_then(|v| query::static_value(&v, "transform:separate_dimensions"))
                .and_then(|v| v.as_bool())
                == Some(true);
            if separate {
                push("transform:position_x", Value::Float(p.x));
                push("transform:position_y", Value::Float(p.y));
            } else {
                push("transform:position", Value::Vec3([p.x, p.y, p.z]));
            }
        }
    }
    out
}

/// Handle a motion action. Returns `false` for ids it does not know.
pub(crate) fn handle(inv: &Invocation, cx: &mut PanelContext<'_>, session: &SharedSession) -> bool {
    let stage = cx.stage;
    let id = inv.id.as_str();
    if !id.starts_with("motion.") && id != ids::COPY && id != ids::PASTE && id != ids::CUT {
        return false;
    }
    if id == ids::CUT {
        // AE: Cut = Copy + Clear (one undo step: the Clear).
        if handle(&Invocation::new(ids::COPY), cx, session) {
            handle(&Invocation::new(ids::DELETE), cx, session);
        }
        return true;
    }
    match id {
        ids::NEW_COMP => {
            common::new_comp(cx, session, None);
            return true;
        }
        ids::COMP_SETTINGS => {
            if let Some(comp) = settings_target(cx, session) {
                crate::dialogs::comp_settings(cx, &comp);
            }
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
            // AE: Solid, Camera and Light Settings first; OK creates the layer.
            let dialog = match kind {
                LayerKind::Solid => Some(crate::dialogs::new_solid as fn(&_, &_)),
                LayerKind::Camera => Some(crate::dialogs::new_camera as fn(&_, &_)),
                LayerKind::Light => Some(crate::dialogs::new_light as fn(&_, &_)),
                _ => None,
            };
            if let Some(open) = dialog {
                open(cx, session);
                return true;
            }
            common::new_layer(cx, session, kind, extra);
            return true;
        }
        ids::IMPORT_FILE => {
            crate::import::import_file(cx, session);
            return true;
        }
        ids::ADD_EXPRESSION => {
            // AE toggles: the selected properties get their default expression;
            // when all of them have one, the expressions are removed.
            let props: Vec<(PrimPath, String)> = session
                .lock()
                .selected_props
                .iter()
                .filter(|(p, a)| {
                    query::locked_layer_of(stage, p).is_none()
                        && stage
                            .prim(p)
                            .is_some_and(|v| bae_model::schema::is_animatable(v.prim_type(), a))
                })
                .cloned()
                .collect();
            let has = |p: &PrimPath, a: &str| {
                query::attr_spec(stage, p, a).is_some_and(|s| s.expression.is_some())
            };
            let remove = props.iter().all(|(p, a)| has(p, a));
            let items: Vec<_> = props
                .iter()
                .filter(|(p, a)| remove || !has(p, a))
                .map(|(p, a)| {
                    let source = (!remove).then(|| crate::expr::default_expression(stage, p, a));
                    json!({"path": p, "attr": a, "source": source})
                })
                .collect();
            if !items.is_empty() {
                cx.invoke_with("attr.set_expressions", json!({"items": items}));
            }
            return true;
        }
        ids::IMPORT_MULTIPLE => {
            crate::import::import_multiple(cx, session);
            return true;
        }
        ids::PRE_COMPOSE => {
            // The selected layers; locked ones cannot be pre-composed (AE).
            let layers: Vec<PrimPath> = session
                .lock()
                .selected_layers
                .iter()
                .filter(|l| !query::is_locked(cx.stage, l))
                .cloned()
                .collect();
            crate::dialogs::precompose(cx, layers);
            return true;
        }
        ids::LAYER_SETTINGS => {
            let project = cx.panel.kind() == "project";
            if let Some(target) = layer_settings_target(cx.stage, session, project) {
                crate::dialogs::layer_settings(cx, session, &target);
            }
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
                s.stop_preview(&info);
            } else {
                s.play(&info);
            }
        }
        ids::GRAPH_EDITOR => s.graph_editor = !s.graph_editor,
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
            // Preferences > Composition > Create Split Layers Above Original
            // Layer (AE default on).
            let above = cx
                .resources
                .get::<std::sync::Arc<bae_prefs::PrefsStore>>()
                .is_none_or(|p| p.current().bool(bae_prefs::keys::SPLIT_LAYERS_ABOVE));
            cx.invoke_with(
                "layer.split",
                json!({"layers": layers, "time": now, "above": above}),
            );
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
        ids::SELECT_KEY_LABEL_GROUP if !keys.is_empty() => {
            // The labels of the selected keyframes, on the selected layers or all.
            let mut labels: Vec<u8> = keys
                .iter()
                .filter_map(|k| {
                    query::attr_spec(stage, &k.path, &k.attr)
                        .and_then(|a| a.keyframes.get(&k.time))
                        .map(|kf| kf.label)
                })
                .filter(|l| *l != 0)
                .collect();
            labels.sort_unstable();
            labels.dedup();
            let scope = inv
                .params
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let on: Vec<PrimPath> = if scope.ends_with("all_layers") {
                query::comp_layers(stage, &info.path)
            } else {
                layers.clone()
            };
            if !labels.is_empty() {
                s.selected_keys = crate::timeline::keys_with_labels(stage, &on, &labels);
            }
        }
        ids::KEY_INTERPOLATION if !keys.is_empty() => {
            s.key_dialog = Some(crate::timeline::KeyDialog::Interpolation(
                crate::timeline::InterpolationDialog::new(keys),
            ));
        }
        ids::KEY_VELOCITY if !keys.is_empty() => {
            s.key_dialog = crate::timeline::VelocityDialog::new(stage, keys)
                .map(crate::timeline::KeyDialog::Velocity);
        }
        ids::ANIMATE_TEXT | ids::ADD_TEXT_SELECTOR | ids::REMOVE_ALL_TEXT_ANIMATORS => {
            let texts: Vec<PrimPath> = layers
                .iter()
                .filter(|l| query::layer_kind(stage, l).as_deref() == Some("text"))
                .cloned()
                .collect();
            if texts.is_empty() {
                return true;
            }
            match id {
                ids::ANIMATE_TEXT => {
                    let property = inv
                        .params
                        .get("property")
                        .cloned()
                        .unwrap_or(json!("position"));
                    cx.invoke_with(
                        "text.animate",
                        json!({"layers": texts, "property": property}),
                    );
                }
                ids::ADD_TEXT_SELECTOR => {
                    if let Some(a) =
                        bae_motion::text_animators::animators_of(stage, &texts[0]).last()
                    {
                        cx.invoke_with("text.add_selector", json!({ "animator": a }));
                    }
                }
                _ => cx.invoke_with("text.remove_animators", json!({ "layers": texts })),
            }
        }
        ids::SET_COMP_MARKER | ids::GO_TO_COMP_MARKER => {
            let n = inv
                .params
                .get("number")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let numbered = numbered_comp_marker(stage, &info.path, n);
            if id == ids::GO_TO_COMP_MARKER {
                if let Some((_, t)) = numbered {
                    s.stop();
                    s.set_time(&info, t);
                }
            } else {
                match numbered {
                    Some((m, t)) if t != now => {
                        cx.invoke_with("marker.edit", json!({"path": m, "time": now}));
                    }
                    Some(_) => {}
                    None => cx.invoke_with(
                        "marker.add",
                        json!({"owner": info.path, "time": now, "comment": n.to_string()}),
                    ),
                }
            }
        }
        ids::ADD_MARKER => {
            if layers.is_empty() {
                cx.invoke_with("marker.add", json!({"owner": info.path, "time": now}));
            } else {
                cx.invoke_with("marker.add", json!({"owners": layers, "comp_time": now}));
            }
        }
        ids::ADD_MARKER_DIALOG => {
            let owner = layers.first().cloned().unwrap_or_else(|| info.path.clone());
            s.key_dialog = Some(crate::timeline::KeyDialog::Marker(Box::new(
                crate::timeline::MarkerDialog::new_marker(&info, owner, now),
            )));
        }
        ids::DELETE_ALL_MARKERS if !layers.is_empty() => {
            let all: Vec<PrimPath> = layers
                .iter()
                .flat_map(|l| bae_motion::markers::markers_of(stage, l))
                .map(|(p, _)| p)
                .collect();
            if !all.is_empty() {
                cx.invoke_with("marker.delete", json!({ "markers": all }));
            }
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
            } else if what == "modified" {
                // Animation ▸ Reveal All Modified Properties (UU).
                Some(Reveal::Modified)
            } else if what == "animation" {
                // Animation ▸ Reveal Properties with Animation: keyframes or
                // expressions, without UU's double-press.
                Some(Reveal::Animated)
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
        ids::BLENDING_MODE if !layers.is_empty() => {
            if let Some(mode) = inv.params.get("mode").and_then(|v| v.as_str()) {
                cx.invoke_with(
                    "layer.set_attr",
                    json!({"layers": layers, "attr": "blend_mode", "value": {"type": "enum", "value": mode}}),
                );
            }
        }
        ids::BLENDING_MODE_STEP if !layers.is_empty() => {
            // Each layer steps from its own mode through AE's menu order.
            let step = inv.params.get("step").and_then(|v| v.as_i64()).unwrap_or(1);
            let modes = bae_model::schema::BLEND_MODES;
            let mut by_mode: Vec<(&str, Vec<PrimPath>)> = Vec::new();
            for l in &layers {
                let current = stage
                    .prim(l)
                    .and_then(|v| query::static_value(&v, "blend_mode"))
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "normal".into());
                let i = modes.iter().position(|m| *m == current).unwrap_or(0) as i64;
                let next = modes[(i + step).rem_euclid(modes.len() as i64) as usize];
                match by_mode.iter_mut().find(|(m, _)| *m == next) {
                    Some((_, ls)) => ls.push(l.clone()),
                    None => by_mode.push((next, vec![l.clone()])),
                }
            }
            for (mode, ls) in by_mode {
                cx.invoke_with(
                    "layer.set_attr",
                    json!({"layers": ls, "attr": "blend_mode", "value": {"type": "enum", "value": mode}}),
                );
            }
        }
        ids::TRACK_MATTE if !layers.is_empty() => {
            // "source": "above" / "below" picks the adjacent layer of each layer.
            let matte = inv
                .params
                .get("matte")
                .and_then(|v| v.as_str())
                .unwrap_or("none");
            let all = query::comp_layers(stage, &info.path);
            let side = inv.params.get("source").and_then(|v| v.as_str());
            for l in &layers {
                let mut p = json!({"layers": [l], "matte": matte});
                let i = all.iter().position(|x| x == l).unwrap_or(0);
                let src = match side {
                    Some("above") => i.checked_sub(1).and_then(|j| all.get(j)),
                    Some("below") => all.get(i + 1),
                    _ => None,
                };
                if let Some(s) = src {
                    p["source"] = json!(s);
                }
                cx.invoke_with("layer.set_track_matte", p);
            }
        }
        ids::LAYER_SWITCH => {
            let Some(attr) = inv.params.get("switch").and_then(|v| v.as_str()) else {
                return true;
            };
            if !SWITCHES.iter().any(|(a, _)| *a == attr) {
                return true;
            }
            // Lock acts on the selected layers even when they are locked.
            let targets = if attr == "locked" {
                s.selected_layers.clone()
            } else {
                layers.clone()
            };
            if targets.is_empty() {
                return true;
            }
            // AE: on for all unless every one is on already.
            let on = !targets.iter().all(|l| switch_on(stage, l, attr));
            cx.invoke_with(
                "layer.set_attr",
                json!({"layers": targets, "attr": attr, "value": {"type": "bool", "value": on}}),
            );
        }
        ids::UNLOCK_ALL_LAYERS | ids::SHOW_ALL_VIDEO | ids::HIDE_OTHER_VIDEO => {
            let all = query::comp_layers(stage, &info.path);
            let (attr, on, targets): (&str, bool, Vec<PrimPath>) = match id {
                ids::UNLOCK_ALL_LAYERS => (
                    "locked",
                    false,
                    all.into_iter()
                        .filter(|l| switch_on(stage, l, "locked"))
                        .collect(),
                ),
                ids::SHOW_ALL_VIDEO => (
                    "enabled",
                    true,
                    all.into_iter()
                        .filter(|l| !switch_on(stage, l, "enabled"))
                        .collect(),
                ),
                _ => (
                    "enabled",
                    false,
                    all.into_iter()
                        .filter(|l| !layers.contains(l) && switch_on(stage, l, "enabled"))
                        .collect(),
                ),
            };
            if !targets.is_empty() && (id != ids::HIDE_OTHER_VIDEO || !layers.is_empty()) {
                cx.invoke_with(
                    "layer.set_attr",
                    json!({"layers": targets, "attr": attr, "value": {"type": "bool", "value": on}}),
                );
            }
        }
        ids::FLIP_HORIZONTAL
        | ids::FLIP_VERTICAL
        | ids::CENTER_IN_VIEW
        | ids::FIT_TO_COMP
        | ids::FIT_TO_COMP_WIDTH
        | ids::FIT_TO_COMP_HEIGHT => {
            let edits = transform_edits(stage, &info, now, &layers, id);
            if !edits.is_empty() {
                cx.invoke_with("attr.set_many", json!({ "edits": edits }));
            }
        }
        ids::QUALITY => {
            let quality = inv
                .params
                .get("quality")
                .and_then(|v| v.as_str())
                .unwrap_or("best");
            let targets: Vec<PrimPath> = layers
                .iter()
                .filter(|l| quality_of(stage, l) != quality)
                .cloned()
                .collect();
            if !targets.is_empty() {
                cx.invoke_with(
                    "layer.set_attr",
                    json!({"layers": targets, "attr": "quality", "value": {"type": "enum", "value": quality}}),
                );
            }
        }
        ids::LABEL => {
            let label = inv
                .params
                .get("label")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .clamp(0, 16);
            if !layers.is_empty() {
                cx.invoke_with(
                    "layer.set_attr",
                    json!({"layers": layers, "attr": "label", "value": {"type": "int", "value": label}}),
                );
            }
        }
        ids::SELECT_LABEL_GROUP => {
            // AE: selects every layer with the label of a selected one.
            let labels: Vec<i64> = layers.iter().map(|l| label_of(stage, l)).collect();
            if !labels.is_empty() {
                let group: Vec<PrimPath> = query::comp_layers(stage, &info.path)
                    .into_iter()
                    .filter(|l| labels.contains(&label_of(stage, l)) && !query::is_locked(stage, l))
                    .collect();
                s.selected_layers = group;
                s.selected_keys.clear();
            }
        }
        ids::COPY if keys.is_empty() => {
            // Layers: everything they hold, so Paste works after a Cut and in
            // another comp.
            if layers.is_empty() {
                return false;
            }
            s.layer_clipboard = bae_motion::copy_layers(stage, &layers);
            s.clipboard = None;
        }
        ids::COPY => {
            let Some(first) = keys.first() else {
                return false;
            };
            s.layer_clipboard.clear();
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
        ids::ENABLE_EXPRESSIONS | ids::DISABLE_EXPRESSIONS => {
            let enabled = id == ids::ENABLE_EXPRESSIONS;
            let mut items = Vec::new();
            for l in layers.iter().filter(|l| !query::is_locked(stage, l)) {
                for n in crate::props::revealed(stage, l, &Reveal::Animated) {
                    if let crate::props::Node::Prop { path, attr, .. } = n
                        && let Some(e) =
                            query::attr_spec(stage, &path, &attr).and_then(|s| s.expression.clone())
                        && e.enabled != enabled
                    {
                        items.push(json!({"path": path, "attr": attr, "source": e.source, "enabled": enabled}));
                    }
                }
            }
            if !items.is_empty() {
                cx.invoke_with("attr.set_expressions", json!({"items": items}));
            }
        }
        ids::REVEAL_EXPRESSION_ERRORS => {
            let mut by_layer: Vec<(PrimPath, Vec<(PrimPath, String)>)> = Vec::new();
            for e in s.expr_errors.clone() {
                let Some(l) = query::owning_layer(stage, &e.prim) else {
                    continue;
                };
                let prop = (e.prim, e.attr);
                match by_layer.iter_mut().find(|(x, _)| *x == l) {
                    Some((_, props)) => props.push(prop),
                    None => by_layer.push((l, vec![prop])),
                }
            }
            for (l, props) in by_layer {
                s.reveal.insert(l, Reveal::Props(props));
            }
        }
        ids::CONVERT_EXPRESSION => {
            // Every frame of the layer's span in the comp becomes a keyframe.
            let props: Vec<(PrimPath, String)> = s
                .selected_props
                .iter()
                .filter(|(p, a)| {
                    query::locked_layer_of(stage, p).is_none()
                        && query::attr_spec(stage, p, a)
                            .and_then(|s| s.expression.as_ref())
                            .is_some_and(|e| e.enabled)
                })
                .cloned()
                .collect();
            let mut eval = bae_anim::Evaluator::new();
            for (path, attr) in props {
                let Some(layer) = query::owning_layer(stage, &path) else {
                    continue;
                };
                let timing = query::layer_timing(stage, &layer);
                let end = timing.out_point.max(timing.in_point).min(info.duration);
                let mut t = timing.in_point.min(timing.out_point).max(Time::ZERO);
                let mut keys = Vec::new();
                while t < end {
                    if let Some(v) = common::current_value(&mut eval, stage, &path, &attr, t) {
                        keys.push(json!({"time": query::to_key_time(stage, &path, t), "value": v}));
                    }
                    t += frame;
                }
                cx.invoke_with(
                    "attr.bake_expression",
                    json!({"path": path, "attr": attr, "keys": keys}),
                );
            }
        }
        ids::PASTE if !s.layer_clipboard.is_empty() => {
            // Above the topmost selected layer, else on top; named like
            // Duplicate; the pasted layers end up selected.
            let before = query::comp_layers(stage, &info.path);
            let above = before
                .iter()
                .find(|l| s.selected_layers.contains(l))
                .cloned();
            cx.invoke_with(
                "layer.paste",
                json!({"comp": info.path, "layers": s.layer_clipboard, "above": above}),
            );
            s.expect(Pending::NewLayers {
                comp: info.path.clone(),
                before,
            });
        }
        ids::PASTE | ids::PASTE_REVERSED => {
            let Some(clip) = s.clipboard.clone() else {
                return id == ids::PASTE_REVERSED;
            };
            let reversed = id == ids::PASTE_REVERSED;
            for l in &layers {
                let kt = query::to_key_time(stage, l, now);
                cx.invoke_with(
                    "key.paste",
                    json!({"path": l, "attr": clip.attr, "time": kt, "keys": clip.keys, "reversed": reversed}),
                );
            }
        }
        // This module's actions are handled even when there is nothing to do
        // (e.g. no keys selected); others go on to their handlers (tools, menus).
        id => return ACTIONS.iter().any(|(a, _)| *a == id),
    }
    true
}

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    // Variants of motion actions and commands (`actions::variants`, #140): the
    // Keyboard Shortcuts window, Edit > Undo and History name each one.
    ("Back 10 Frames", "后退 10 帧"),
    ("Forward 10 Frames", "前进 10 帧"),
    ("New Solid", "新建纯色"),
    ("New Adjustment Layer", "新建调整图层"),
    ("New Null Object", "新建空对象"),
    ("New Text Layer", "新建文本图层"),
    ("New Camera", "新建摄像机"),
    ("New Light", "新建灯光"),
    ("New Shape Layer", "新建形状图层"),
    ("New Rectangle", "新建矩形"),
    ("New Ellipse", "新建椭圆"),
    // Layer menu actions and their variants (Keyboard Shortcuts, Undo).
    ("Layer Switch", "图层开关"),
    ("Unlock All Layers", "解锁所有图层"),
    ("Show All Video", "显示所有视频"),
    ("Hide Other Video", "隐藏其他视频"),
    ("Quality", "品质"),
    ("Best Quality", "最佳品质"),
    ("Draft Quality", "草图品质"),
    ("Wireframe Quality", "线框品质"),
    ("Show Position", "显示位置"),
    ("Add Position to Shown Properties", "将位置添加到显示的属性"),
    ("Position: Add or Remove Keyframe", "位置：添加或移除关键帧"),
    ("Show Scale", "显示缩放"),
    ("Add Scale to Shown Properties", "将缩放添加到显示的属性"),
    ("Scale: Add or Remove Keyframe", "缩放：添加或移除关键帧"),
    ("Show Rotation", "显示旋转"),
    ("Add Rotation to Shown Properties", "将旋转添加到显示的属性"),
    ("Rotation: Add or Remove Keyframe", "旋转：添加或移除关键帧"),
    ("Show Opacity", "显示不透明度"),
    (
        "Add Opacity to Shown Properties",
        "将不透明度添加到显示的属性",
    ),
    (
        "Opacity: Add or Remove Keyframe",
        "不透明度：添加或移除关键帧",
    ),
    ("Show Anchor Point", "显示锚点"),
    (
        "Add Anchor Point to Shown Properties",
        "将锚点添加到显示的属性",
    ),
    (
        "Anchor Point: Add or Remove Keyframe",
        "锚点：添加或移除关键帧",
    ),
    // Edit / layer commands
    ("Duplicate", "重复"),
    ("Split Layer", "拆分图层"),
    ("Select All", "全选"),
    ("Deselect All", "全部取消选择"),
    ("Copy", "复制"),
    ("Paste", "粘贴"),
    // Layer > Arrange: AE 26 and the usual Chinese UI wording (docs/glossary.md).
    ("Bring Layer Forward", "前移一层"),
    ("Send Layer Backward", "后移一层"),
    ("Bring Layer to Front", "置于顶层"),
    ("Send Layer to Back", "置于底层"),
    ("Trim Layer In Point", "将入点修剪到当前时间"),
    ("Trim Layer Out Point", "将出点修剪到当前时间"),
    ("Move In Point to Current Time", "将入点移到当前时间"),
    ("Move Out Point to Current Time", "将出点移到当前时间"),
    ("Set Work Area Start", "将工作区域开头设置为当前时间"),
    ("Set Work Area End", "将工作区域结尾设置为当前时间"),
    ("Trim Comp to Work Area", "将合成修剪至工作区域"),
    // Layer > Transform (AE 26's Chinese menu).
    ("Flip Horizontal", "水平翻转"),
    ("Flip Vertical", "垂直翻转"),
    ("Center In View", "视点居中"),
    ("Fit to Comp", "适合复合"),
    ("Fit to Comp Width", "适合复合宽度"),
    ("Fit to Comp Height", "适合复合高度"),
];

/// Registers this module's Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
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
