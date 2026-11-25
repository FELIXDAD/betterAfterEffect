//! Resolution: the comp's or Auto, nested comps, shortcuts.

use super::*;

#[test]
fn the_viewer_renders_at_the_comps_resolution_or_auto() {
    let mut h = H::new();
    assert_eq!(h.viewer.render_factor, 1);
    // View > Resolution > Half (Cmd/Ctrl+Shift+J): the comp's resolution, which
    // Composition Settings shows too.
    let r = tool_action(&h, crate::tools::ids::RESOLUTION, json!({"factor": 2}));
    let p = one(&r, "comp.set_settings");
    assert_eq!(p["resolution"], json!([2, 2]));
    h.apply(&r);
    assert_eq!(
        h.value(&h.comp, "resolution"),
        Some(Value::Vec2([2.0, 2.0]))
    );
    assert_eq!(h.viewer.render_factor, 2);
    // Composition Settings changing it: the viewer follows.
    h.run(
        "comp.set_settings",
        json!({"comp": h.comp, "resolution": [3, 3]}),
    );
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.viewer.render_factor, 3);
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.viewer.render_factor, 2);
    // Auto: the step the magnification hides.
    h.session().lock().auto_resolution = true;
    for (zoom, factor) in [(1.0, 1), (0.5, 2), (0.333, 3), (0.25, 4), (2.0, 1)] {
        h.viewer.zoom = Some(zoom);
        h.frame(Vec::new(), Modifiers::NONE);
        assert_eq!(h.viewer.render_factor, factor, "{zoom}");
    }
    // Choosing a resolution turns Auto off.
    let r = tool_action(&h, crate::tools::ids::RESOLUTION, json!({"factor": 1}));
    h.apply(&r);
    assert!(!h.session().lock().auto_resolution);
    assert_eq!(h.viewer.render_factor, 1);
}

#[test]
fn resolution_recurse_sets_nested_comps_and_ae_shortcuts_are_bound() {
    let mut h = H::new();
    let inner = path(&h.run("comp.create", json!({"width": 100, "height": 100}))["path"]);
    h.run(
        "layer.create",
        json!({"comp": h.comp, "kind": "pre_comp", "source": inner}),
    );
    let r = tool_action(
        &h,
        crate::tools::ids::RESOLUTION,
        json!({"factor": 4, "nested": true}),
    );
    assert_eq!(r.invocations.len(), 2, "{:?}", r.invocations);
    h.apply(&r);
    assert_eq!(h.value(&inner, "resolution"), Some(Value::Vec2([4.0, 4.0])));
    // AE's shortcuts bind once the action has a handler (Cmd/Ctrl+J, +Shift, …).
    use bae_ui::ActionHandler as _;
    let handled: std::collections::HashSet<String> =
        crate::tools::ToolActions.actions().into_iter().collect();
    let late = crate::actions::late_bindings(&handled, &[]);
    let half = late
        .iter()
        .find(|b| b.action == crate::tools::ids::RESOLUTION && b.params == json!({"factor": 2}))
        .expect("Half bound");
    assert_eq!(half.chord.to_string(), "Cmd+Shift+J");
}
