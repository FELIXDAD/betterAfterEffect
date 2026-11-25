//! Toolbar tools (workstream 12 A): choosing tools by click, flyout, Alt-click and
//! AE's shortcuts; spring-loaded keys; Space to pan; Hand and Zoom tools in the
//! Composition viewer; double-clicking tool buttons; tools that do nothing in the
//! viewer yet.

use bae_model::{PrimPath, Value};
use bae_motion::query;
use bae_ui_motion::{SharedTools, Tool};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::kittest::Queryable as _;

use super::*;

fn tools(h: &Ui) -> SharedTools {
    h.state()
        .app
        .resource::<SharedTools>()
        .cloned()
        .expect("the motion module registers the tools")
}

fn tool(h: &Ui) -> Tool {
    tools(h).current()
}

/// The Composition viewer's image area.
fn canvas(h: &Ui) -> Rect {
    h.root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer")
}

/// The toolbar button (or flyout row) named `label`, topmost first.
fn button(h: &Ui, label: &str) -> Rect {
    h.root()
        .query_all_by_label(label)
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("no “{label}”"))
}

fn key_down(h: &mut Ui, modifiers: Modifiers, key: Key, pressed: bool) {
    send(
        h,
        Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        },
    );
}

fn position(h: &Ui, layer: &PrimPath) -> [f64; 3] {
    match query::static_value(
        &h.state().stage().prim(layer).unwrap(),
        "transform:position",
    ) {
        Some(Value::Vec3(v)) => v,
        other => panic!("position {other:?}"),
    }
}

/// The magnification dropdown's text ("Fit", "50%" …).
fn magnification(h: &Ui) -> String {
    let labels = [
        "Fit", "12.5%", "25%", "33.3%", "50%", "100%", "200%", "400%",
    ];
    labels
        .into_iter()
        .find(|l| h.root().query_by_label(l).is_some())
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn ae_tool_shortcuts_choose_and_cycle_tools() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    assert_eq!(tool(&h), Tool::Selection);
    let none = Modifiers::NONE;
    for (m, k, want) in [
        (none, Key::H, Tool::Hand),
        (none, Key::Z, Tool::Zoom),
        (none, Key::W, Tool::Rotation),
        (none, Key::Y, Tool::PanBehind),
        (none, Key::V, Tool::Selection),
        (none, Key::Q, Tool::Rectangle),
        (none, Key::Q, Tool::RoundedRectangle),
        (Modifiers::SHIFT, Key::Q, Tool::Ellipse),
        (none, Key::G, Tool::Pen),
        (none, Key::G, Tool::MaskFeather),
        (Modifiers::COMMAND, Key::T, Tool::TypeHorizontal),
        (Modifiers::COMMAND, Key::T, Tool::TypeVertical),
        (Modifiers::COMMAND, Key::B, Tool::Brush),
        (Modifiers::COMMAND, Key::B, Tool::CloneStamp),
        (Modifiers::COMMAND, Key::P, Tool::PuppetPosition),
        (Modifiers::ALT, Key::W, Tool::RotoBrush),
        (none, Key::C, Tool::OrbitCursor),
        (none, Key::C, Tool::PanCursor),
        (none, Key::Num3, Tool::DollyCursor),
        (Modifiers::SHIFT, Key::Num1, Tool::OrbitCursor),
        (Modifiers::SHIFT, Key::Num1, Tool::OrbitScene),
        (Modifiers::SHIFT, Key::D, Tool::Cube),
        (none, Key::V, Tool::Selection),
        // Q returns to the shape tool used last.
        (none, Key::Q, Tool::Ellipse),
    ] {
        key(&mut h, m, k);
        assert_eq!(tool(&h), want, "{m:?}+{k:?}");
    }
    // Tool shortcuts are in the Keyboard Shortcuts list of the AE keymap.
    let km = h.state().workspace.keymap().clone();
    assert!(
        km.chords_for("motion.tool.pen")
            .any(|c| c.to_string() == "G")
    );
    assert!(
        km.chords_for("motion.tool.type")
            .any(|c| c.to_string() == "Cmd+T")
    );
}

#[test]
fn toolbar_buttons_flyouts_and_alt_click() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    // Plain click.
    let at = button(&h, "Hand Tool").center();
    click_at(&mut h, at);
    assert_eq!(tool(&h), Tool::Hand);
    // Alt/Option-click on a group: its next tool.
    pause(&mut h);
    let at = button(&h, "Rectangle Tool").center();
    click_at_mods(&mut h, at, Modifiers::ALT);
    assert_eq!(tool(&h), Tool::RoundedRectangle);
    // Right-click opens the flyout with every tool of the group.
    pause(&mut h);
    let group = button(&h, "Rounded Rectangle Tool");
    send(&mut h, Event::PointerMoved(group.center()));
    for pressed in [true, false] {
        send(
            &mut h,
            Event::PointerButton {
                pos: group.center(),
                button: PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        );
    }
    settle(&mut h);
    for name in [
        "Rectangle Tool",
        "Ellipse Tool",
        "Polygon Tool",
        "Star Tool",
    ] {
        assert!(
            h.root().query_by_label(name).is_some(),
            "flyout lists {name}"
        );
    }
    let at = button(&h, "Star Tool").center();
    click_at(&mut h, at);
    assert_eq!(tool(&h), Tool::Star);
    assert!(
        h.root().query_by_label("Ellipse Tool").is_none(),
        "flyout closed"
    );
    // Press and hold opens the flyout too.
    pause(&mut h);
    let pen = button(&h, "Pen Tool");
    send(&mut h, Event::PointerMoved(pen.center()));
    send(
        &mut h,
        Event::PointerButton {
            pos: pen.center(),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    );
    for _ in 0..30 {
        frame(&mut h);
    }
    assert!(
        h.root().query_by_label("Convert Vertex Tool").is_some(),
        "holding the Pen button opens its flyout"
    );
    send(
        &mut h,
        Event::PointerButton {
            pos: pen.center(),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    );
    settle(&mut h);
    let at = button(&h, "Convert Vertex Tool").center();
    click_at(&mut h, at);
    assert_eq!(tool(&h), Tool::ConvertVertex);
    // The tool is saved with the workbench state (application state).
    let state = h.state_mut().workspace.state();
    let saved = state.panels.get("toolbar:ae").expect("toolbar state saved");
    assert_eq!(saved["tool"], "convert_vertex");
}

#[test]
fn shape_and_pen_tools_show_their_options() {
    let mut h = gui();
    comp_with(&mut h, &[]);
    key(&mut h, Modifiers::NONE, Key::Q);
    for label in ["Fill Color", "Stroke Color", "Stroke Width", "Bezier Path"] {
        assert!(h.root().query_by_label(label).is_some(), "{label} shown");
    }
    assert!(h.root().query_by_label("Tool Creates Mask").is_none());
    key(&mut h, Modifiers::NONE, Key::G);
    assert!(h.root().query_by_label("RotoBezier").is_some());
    // With a shape layer selected: Tool Creates Shape / Mask.
    assert!(h.state_mut().menu("motion.layer.new.shape"));
    settle(&mut h);
    assert_eq!(h.state().selected_layers().len(), 1);
    assert!(h.root().query_by_label("Tool Creates Shape").is_some());
    let at = button(&h, "Tool Creates Mask").center();
    click_at(&mut h, at);
    assert!(tools(&h).lock().options.creates_mask);
    // The Selection tool shows no paint options.
    key(&mut h, Modifiers::NONE, Key::V);
    assert!(h.root().query_by_label("Fill Color").is_none());
}

#[test]
fn holding_a_tool_key_is_temporary() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let before = position(&h, &layer);
    let c = canvas(&h).center();
    // Hold H, drag in the viewer (pans, does not move the layer), release H.
    key_down(&mut h, Modifiers::NONE, Key::H, true);
    frame(&mut h);
    assert_eq!(tool(&h), Tool::Hand);
    drag(&mut h, c, c + vec2(80.0, 30.0), Modifiers::NONE);
    assert_eq!(
        position(&h, &layer),
        before,
        "the Hand tool does not move layers"
    );
    key_down(&mut h, Modifiers::NONE, Key::H, false);
    settle(&mut h);
    assert_eq!(tool(&h), Tool::Selection, "back to the Selection tool");
    // A tap stays.
    key(&mut h, Modifiers::NONE, Key::H);
    assert_eq!(tool(&h), Tool::Hand);
}

#[test]
fn space_drag_pans_and_does_not_play() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let before = position(&h, &layer);
    let t0 = h.state().now();
    let c = canvas(&h).center();
    send(&mut h, Event::PointerMoved(c));
    key_down(&mut h, Modifiers::NONE, Key::Space, true);
    settle(&mut h);
    drag(&mut h, c, c + vec2(-90.0, 40.0), Modifiers::NONE);
    key_down(&mut h, Modifiers::NONE, Key::Space, false);
    settle(&mut h);
    assert!(!h.state().is_playing(), "a Space-drag only pans");
    assert_eq!(h.state().now(), t0);
    assert_eq!(position(&h, &layer), before);
    assert_eq!(tool(&h), Tool::Selection);
    // Space alone still plays.
    key(&mut h, Modifiers::NONE, Key::Space);
    assert!(h.state().is_playing());
    key(&mut h, Modifiers::NONE, Key::Space);
}

#[test]
fn zoom_tool_clicks_and_double_click_to_100_percent() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    assert_eq!(magnification(&h), "Fit");
    key(&mut h, Modifiers::NONE, Key::Z);
    let c = canvas(&h).center();
    click_at(&mut h, c);
    let zoomed_in = magnification(&h);
    assert!(zoomed_in.ends_with('%'), "zoomed in: {zoomed_in}");
    pause(&mut h);
    click_at_mods(&mut h, c, Modifiers::ALT);
    pause(&mut h);
    click_at_mods(&mut h, c, Modifiers::ALT);
    let zoomed_out = magnification(&h);
    assert_ne!(zoomed_out, zoomed_in, "Alt-click zooms out");
    // Double-click the Zoom tool button: 100%.
    let at = button(&h, "Zoom Tool").center();
    double_click_at(&mut h, at);
    assert_eq!(magnification(&h), "100%");
}

#[test]
fn double_click_selection_and_rotation_tools_reset_the_selected_layers() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let set = |h: &mut Ui, attr: &str, value: Value| {
        h.state_mut().invoke(Invocation::with_params(
            "attr.set",
            serde_json::json!({"path": layer, "attr": attr, "value": value}),
        ));
        settle(h);
    };
    set(&mut h, "transform:scale", Value::Vec3([40.0, 60.0, 100.0]));
    set(&mut h, "transform:rotation", Value::Float(30.0));
    assert_eq!(h.state().selected_layers(), std::slice::from_ref(&layer));
    let at = button(&h, "Selection Tool").center();
    double_click_at(&mut h, at);
    let get =
        |h: &Ui, attr: &str| query::static_value(&h.state().stage().prim(&layer).unwrap(), attr);
    assert_eq!(
        get(&h, "transform:scale"),
        Some(Value::Vec3([100.0, 100.0, 100.0]))
    );
    assert_eq!(get(&h, "transform:rotation"), Some(Value::Float(30.0)));
    let at = button(&h, "Rotation Tool").center();
    double_click_at(&mut h, at);
    assert_eq!(get(&h, "transform:rotation"), Some(Value::Float(0.0)));
    // One undo step each.
    let (m, k) = cmd(Key::Z);
    key(&mut h, m, k);
    assert_eq!(get(&h, "transform:rotation"), Some(Value::Float(30.0)));
}

#[test]
fn tools_without_viewer_actions_do_nothing_there() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let before = position(&h, &layer);
    let c = canvas(&h).center();
    for (m, k) in [
        (Modifiers::COMMAND, Key::B),
        (Modifiers::COMMAND, Key::P),
        (Modifiers::ALT, Key::W),
        (Modifiers::NONE, Key::C),
        (Modifiers::SHIFT, Key::D),
    ] {
        key(&mut h, m, k);
        let t = tool(&h);
        let commands = h.state().model_commands().len();
        click_at(&mut h, c);
        drag(&mut h, c, c + vec2(50.0, 50.0), Modifiers::NONE);
        assert_eq!(tool(&h), t);
        assert_eq!(
            h.state().model_commands().len(),
            commands,
            "{t:?} changed the project"
        );
        assert_eq!(position(&h, &layer), before);
    }
}
