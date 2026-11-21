//! The motion workspace's menu bar entries (AE's Composition, Layer, Animation and
//! Edit items) and its global action handler, so menus and AE shortcuts work
//! whichever panel has focus.

use bae_ui::menu::{MenuContext, MenuItemSpec};
use bae_ui::{ActionHandler, Invocation, PanelContext};
use serde_json::json;

use crate::actions::{self, ids};
use crate::common;

/// Workspace id of the motion workspace (priority for conflicts).
pub const WORKSPACE: &str = "motion";

fn has_comp(cx: &MenuContext<'_>) -> bool {
    common::active_comp(cx.stage, &common::session(cx.resources)).is_some()
}

fn has_layers(cx: &MenuContext<'_>) -> bool {
    has_comp(cx)
        && !common::session(cx.resources)
            .lock()
            .selected_layers
            .is_empty()
}

/// Layer Settings has a target: a selected layer that uses a solid, or a
/// solid item selected in the Project panel.
fn has_layer_settings(cx: &MenuContext<'_>) -> bool {
    let session = common::session(cx.resources);
    actions::layer_settings_target(cx.stage, &session, true).is_some()
}

fn has_keys(cx: &MenuContext<'_>) -> bool {
    has_comp(cx)
        && !common::session(cx.resources)
            .lock()
            .selected_keys
            .is_empty()
}

/// `(registry id, item)` of every motion menu item. Each runs the same invocation
/// its AE shortcut binding carries, so the menu shows that shortcut.
pub fn items() -> Vec<(&'static str, MenuItemSpec)> {
    let item = |path: &[&str], title: &str, action: &str| {
        MenuItemSpec::new(path, title, action).workspace(WORKSPACE)
    };
    let new_layer = |title: &str, kind: &str, order: i32| {
        item(&["Layer", "New"], title, ids::NEW_LAYER)
            .params(json!({ "kind": kind }))
            .order(order)
            .enabled_when(has_comp)
    };
    let arrange = |title: &str, to: &str, order: i32| {
        item(&["Layer", "Arrange"], title, ids::ORDER)
            .params(json!({ "to": to }))
            .group(2)
            .order(order)
            .enabled_when(has_layers)
    };
    let ease = |title: &str, mode: &str, order: i32| {
        item(&["Animation", "Keyframe Assistant"], title, ids::EASE)
            .params(json!({ "mode": mode }))
            .order(order)
            .enabled_when(has_keys)
    };
    vec![
        // Composition
        (
            "motion.composition.new",
            item(&["Composition"], "New Composition...", ids::NEW_COMP),
        ),
        (
            "motion.comp.composition_settings",
            item(
                &["Composition"],
                "Composition Settings...",
                ids::COMP_SETTINGS,
            )
            .order(1)
            .enabled_when(has_comp),
        ),
        (
            "motion.composition.trim",
            item(&["Composition"], "Trim Comp to Work Area", ids::TRIM_COMP)
                .group(1)
                .enabled_when(has_comp),
        ),
        (
            "motion.composition.render_queue",
            item(
                &["Composition"],
                "Add to Render Queue",
                ids::ADD_TO_RENDER_QUEUE,
            )
            .group(2)
            .enabled_when(has_comp),
        ),
        // Layer ▸ New (AE order), then the shape presets.
        ("motion.layer.new.text", new_layer("Text", "text", 1)),
        ("motion.layer.new.solid", new_layer("Solid...", "solid", 2)),
        ("motion.layer.new.light", new_layer("Light", "light", 3)),
        ("motion.layer.new.camera", new_layer("Camera", "camera", 4)),
        ("motion.layer.new.null", new_layer("Null Object", "null", 5)),
        (
            "motion.layer.new.shape",
            new_layer("Shape Layer", "shape", 6),
        ),
        (
            "motion.layer.new.adjustment",
            new_layer("Adjustment Layer", "adjustment", 7),
        ),
        (
            "motion.layer.new.rectangle",
            item(&["Layer", "New"], "Rectangle", ids::NEW_LAYER)
                .params(json!({ "kind": "shape", "shape": "rect" }))
                .group(1)
                .order(1)
                .enabled_when(has_comp),
        ),
        (
            "motion.layer.new.ellipse",
            item(&["Layer", "New"], "Ellipse", ids::NEW_LAYER)
                .params(json!({ "kind": "shape", "shape": "ellipse" }))
                .group(1)
                .order(2)
                .enabled_when(has_comp),
        ),
        // Layer ▸ Layer Settings (AE names it after the selected layer:
        // Solid Settings…; the menu title is static for now).
        (
            "motion.layer.layer_settings",
            item(&["Layer"], "Layer Settings...", ids::LAYER_SETTINGS)
                .group(1)
                .order(1)
                .enabled_when(has_layer_settings),
        ),
        // Layer ▸ Arrange
        (
            "motion.layer.arrange.front",
            arrange("Bring Layer to Front", "front", 1),
        ),
        (
            "motion.layer.arrange.forward",
            arrange("Bring Layer Forward", "forward", 2),
        ),
        (
            "motion.layer.arrange.backward",
            arrange("Send Layer Backward", "backward", 3),
        ),
        (
            "motion.layer.arrange.back",
            arrange("Send Layer to Back", "back", 4),
        ),
        // Animation
        ("motion.animation.ease", ease("Easy Ease", "easy_ease", 1)),
        (
            "motion.animation.ease_in",
            ease("Easy Ease In", "ease_in", 2),
        ),
        (
            "motion.animation.ease_out",
            ease("Easy Ease Out", "ease_out", 3),
        ),
        (
            "motion.animation.reverse",
            item(
                &["Animation", "Keyframe Assistant"],
                "Time-Reverse Keyframes",
                ids::REVERSE_KEYS,
            )
            .group(1)
            .enabled_when(has_keys),
        ),
        (
            "motion.animation.hold",
            item(&["Animation"], "Toggle Hold Keyframe", ids::TOGGLE_HOLD)
                .group(1)
                .enabled_when(has_keys),
        ),
        // Edit (AE puts Duplicate and Split Layer here).
        (
            "motion.edit.duplicate",
            item(&["Edit"], "Duplicate", ids::DUPLICATE)
                .group(10)
                .order(1)
                .enabled_when(has_layers),
        ),
        (
            "motion.edit.split",
            item(&["Edit"], "Split Layer", ids::SPLIT)
                .group(10)
                .order(2)
                .enabled_when(has_layers),
        ),
        // Window > Render Queue (AE, Cmd/Ctrl+Alt+0): the same invocation as the
        // keymap's panel binding, so the menu shows that shortcut and toggles.
        (
            "motion.window.render_queue",
            MenuItemSpec::new(&["Window"], "Render Queue", bae_ui::action::ids::OPEN_PANEL)
                .params(json!({"id": "render_queue", "toggle": true}))
                .order(75),
        ),
    ]
}

/// Runs the `motion.*` actions whichever panel has focus.
#[derive(Debug, Default)]
pub struct MotionActions;

impl ActionHandler for MotionActions {
    fn actions(&self) -> Vec<String> {
        actions::ACTIONS
            .iter()
            .map(|(id, _)| (*id).to_owned())
            .collect()
    }

    fn workspace(&self) -> Option<&str> {
        Some(WORKSPACE)
    }

    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        actions::handle(inv, cx, &session)
    }
}
