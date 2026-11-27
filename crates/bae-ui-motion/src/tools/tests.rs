//! Tool state: shortcuts, cycling, groups, spring-loaded keys, persistence.

use super::*;

fn press(t: &mut ToolState, action: &str) -> Tool {
    assert!(t.shortcut(action, true), "{action} is a tool shortcut");
    // A tap: the key is up again before the next frame.
    t.spring(|_| false, false);
    t.current()
}

#[test]
fn single_tool_shortcuts() {
    let mut t = ToolState::default();
    assert_eq!(t.current(), Tool::Selection);
    for (action, tool) in [
        (ids::HAND, Tool::Hand),
        (ids::ZOOM, Tool::Zoom),
        (ids::ROTATION, Tool::Rotation),
        (ids::PAN_BEHIND, Tool::PanBehind),
        (ids::SELECTION, Tool::Selection),
    ] {
        assert_eq!(press(&mut t, action), tool, "{action}");
        // Pressing again keeps it.
        assert_eq!(press(&mut t, action), tool, "{action} again");
    }
    assert!(!t.shortcut("motion.play", true));
}

#[test]
fn repeated_group_shortcuts_cycle() {
    let mut t = ToolState::default();
    let shapes: Vec<Tool> = (0..6).map(|_| press(&mut t, ids::SHAPE)).collect();
    assert_eq!(
        shapes,
        [
            Tool::Rectangle,
            Tool::RoundedRectangle,
            Tool::Ellipse,
            Tool::Polygon,
            Tool::Star,
            Tool::Rectangle
        ]
    );
    assert_eq!(press(&mut t, ids::SHAPE_CYCLE), Tool::RoundedRectangle);
    // G: Pen and Mask Feather only.
    let pens: Vec<Tool> = (0..3).map(|_| press(&mut t, ids::PEN)).collect();
    assert_eq!(pens, [Tool::Pen, Tool::MaskFeather, Tool::Pen]);
    t.set(Tool::ConvertVertex);
    assert_eq!(press(&mut t, ids::PEN), Tool::Pen);
    // Cmd+T, Cmd+B, Cmd+P, Alt+W.
    let types: Vec<Tool> = (0..3).map(|_| press(&mut t, ids::TYPE)).collect();
    assert_eq!(
        types,
        [
            Tool::TypeHorizontal,
            Tool::TypeVertical,
            Tool::TypeHorizontal
        ]
    );
    let paint: Vec<Tool> = (0..4).map(|_| press(&mut t, ids::PAINT)).collect();
    assert_eq!(
        paint,
        [Tool::Brush, Tool::CloneStamp, Tool::Eraser, Tool::Brush]
    );
    assert_eq!(press(&mut t, ids::PUPPET), Tool::PuppetPosition);
    assert_eq!(press(&mut t, ids::PUPPET), Tool::PuppetStarch);
    assert_eq!(press(&mut t, ids::ROTO_BRUSH), Tool::RotoBrush);
    assert_eq!(press(&mut t, ids::ROTO_BRUSH), Tool::RefineEdge);
    assert_eq!(press(&mut t, ids::PARAMETRIC_MESH), Tool::Cube);
    assert_eq!(press(&mut t, ids::PARAMETRIC_MESH), Tool::Sphere);
}

#[test]
fn camera_shortcuts() {
    let mut t = ToolState::default();
    // C: Orbit → Pan → Dolly groups.
    let c: Vec<Tool> = (0..4).map(|_| press(&mut t, ids::CAMERA)).collect();
    assert_eq!(
        c,
        [
            Tool::OrbitCursor,
            Tool::PanCursor,
            Tool::DollyCursor,
            Tool::OrbitCursor
        ]
    );
    // 1 activates the orbit group without cycling; Shift+1 cycles its tools.
    assert_eq!(press(&mut t, ids::CAMERA_ORBIT), Tool::OrbitCursor);
    assert_eq!(press(&mut t, ids::CAMERA_ORBIT), Tool::OrbitCursor);
    assert_eq!(press(&mut t, ids::CAMERA_ORBIT_CYCLE), Tool::OrbitScene);
    assert_eq!(press(&mut t, ids::CAMERA_PAN_CYCLE), Tool::PanCursor);
    assert_eq!(press(&mut t, ids::CAMERA_PAN_CYCLE), Tool::PanPoi);
    assert_eq!(press(&mut t, ids::CAMERA_DOLLY), Tool::DollyCursor);
    // Each group remembers its tool: C goes back to Orbit Around Scene.
    assert_eq!(press(&mut t, ids::CAMERA), Tool::OrbitScene);
    assert_eq!(press(&mut t, ids::CAMERA_PAN), Tool::PanPoi);
}

#[test]
fn groups_remember_their_last_tool() {
    let mut t = ToolState::default();
    t.set(Tool::Ellipse);
    assert_eq!(t.shown(Group::Shape), Tool::Ellipse);
    press(&mut t, ids::SELECTION);
    assert_eq!(
        t.shown(Group::Shape),
        Tool::Ellipse,
        "button keeps showing it"
    );
    assert_eq!(press(&mut t, ids::SHAPE), Tool::Ellipse, "Q returns to it");
    // Alt-click on the group button: the member after the one it shows, all
    // members (the Pen group's vertex tools too).
    assert_eq!(t.next_in_group(Group::Shape, true), Tool::Polygon);
    press(&mut t, ids::SELECTION);
    assert_eq!(t.next_in_group(Group::Shape, true), Tool::Polygon);
    assert_eq!(t.next_in_group(Group::Pen, true), Tool::AddVertex);
    assert_eq!(t.next_in_group(Group::Pen, false), Tool::MaskFeather);
}

#[test]
fn holding_a_tool_key_switches_back_on_release() {
    let mut t = ToolState::default();
    t.set(Tool::Selection);
    // H held, the pointer used, H released: back to Selection.
    assert!(t.shortcut(ids::HAND, true));
    assert_eq!(t.current(), Tool::Hand);
    assert!(t.spring(|a| a == ids::HAND, false));
    assert!(t.spring(|a| a == ids::HAND, true));
    // Key repeat while held: no cycling, no change.
    assert!(t.shortcut(ids::HAND, true));
    assert!(!t.spring(|_| false, false));
    assert_eq!(t.current(), Tool::Selection);

    // Q held and used: the shape tool is temporary too, and key repeat does not
    // cycle the group.
    assert!(t.shortcut(ids::SHAPE, true));
    assert!(t.spring(|a| a == ids::SHAPE, true));
    assert!(t.shortcut(ids::SHAPE, true));
    assert_eq!(t.current(), Tool::Rectangle);
    t.spring(|_| false, false);
    assert_eq!(t.current(), Tool::Selection);

    // Held without a click, but long enough: back as well.
    assert!(t.shortcut(ids::ZOOM, true));
    t.spring(|a| a == ids::ZOOM, false);
    t.momentary.as_mut().unwrap().since -= SPRING_HOLD;
    t.spring(|_| false, false);
    assert_eq!(t.current(), Tool::Selection);

    // A tap latches.
    assert!(t.shortcut(ids::ZOOM, true));
    t.spring(|_| false, false);
    assert_eq!(t.current(), Tool::Zoom);
    // Choosing a tool on the toolbar ends a pending spring.
    assert!(t.shortcut(ids::HAND, true));
    t.spring(|a| a == ids::HAND, true);
    t.set(Tool::Pen);
    t.spring(|_| false, false);
    assert_eq!(t.current(), Tool::Pen);
}

#[test]
fn state_survives_a_restart() {
    let mut t = ToolState::default();
    t.set(Tool::Star);
    t.set(Tool::ConvertVertex);
    t.options.stroke_width = 7.5;
    t.options.fill.kind = PaintKind::None;
    let saved = t.save();
    let mut u = ToolState::default();
    u.load(&saved);
    assert_eq!(u.current(), Tool::ConvertVertex);
    assert_eq!(u.shown(Group::Shape), Tool::Star);
    assert_eq!(u.options, t.options);
    // Garbage is ignored.
    u.load(&serde_json::json!("nope"));
    assert_eq!(u.current(), Tool::ConvertVertex);
}

#[test]
fn every_tool_is_in_one_group_and_named_in_chinese() {
    let mut seen = Vec::new();
    for g in Group::ALL {
        for &t in g.members() {
            assert!(!seen.contains(&t), "{t:?} twice");
            assert_eq!(t.group(), g);
            seen.push(t);
        }
    }
    assert_eq!(seen.len(), 41);
    bae_ui::i18n::register(ZH);
    for t in seen {
        assert!(
            bae_ui::i18n::has_translation(t.name()),
            "no Chinese for {}",
            t.name()
        );
    }
    for (_, title) in actions() {
        assert!(
            bae_ui::i18n::has_translation(title),
            "no Chinese for {title}"
        );
    }
}

#[test]
fn shortcuts_parse_and_do_not_clash() {
    let b = bindings();
    // + Esc (Switch to Last 3D View).
    assert_eq!(b.len(), SHORTCUTS.len() + NUDGES.len() + 1 + GIZMOS.len());
    let mut chords: Vec<String> = b.iter().map(|b| b.chord.to_string()).collect();
    chords.sort();
    chords.dedup();
    assert_eq!(chords.len(), b.len(), "two tool actions share a chord");
}
