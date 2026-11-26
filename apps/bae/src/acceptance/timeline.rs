//! Timeline: select, rename (Enter / double-click, type, Enter / click away /
//! Esc), Chinese through an IME composition, spaces, emoji, long names;
//! stopwatch and Option+Shift+P keyframes, J/K; Space with the focus in a text
//! field, on a button and on nothing; undo / redo of all of it.

use bae_model::Time;
use egui::{Key, Modifiers, Pos2, pos2};
use egui_kittest::kittest::Queryable as _;

use super::*;

const POSITION: &str = "transform:position";

/// The timeline's name column header ("Source Name" or "Layer Name").
fn header(h: &Ui) -> Rect {
    find_texts(h, |t| t == "Source Name" || t == "Layer Name")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no timeline header in {:?}", painted_texts(h)))
}

/// Where the timeline paints `name` (a layer row or a property row), below the
/// column header and in the outline (left of the graph).
fn timeline_text(h: &Ui, name: &str) -> Option<Rect> {
    let head = header(h);
    find_texts(h, |t| t == name)
        .into_iter()
        .find(|r| r.top() > head.bottom() && r.left() < head.left() + 400.0)
}

fn layer_row(h: &Ui, name: &str) -> Pos2 {
    timeline_text(h, name)
        .unwrap_or_else(|| panic!("no timeline row {name:?} in {:?}", painted_texts(h)))
        .center()
}

/// Comp with one solid, the solid selected; returns the harness.
fn comp_with_solid() -> Ui {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    assert_eq!(h.state().layer_names(), ["Dark Gray Solid 1"]);
    h
}

/// The rename field, if one is open (the only single-line text input in the
/// timeline outline).
fn rename_field_value(h: &Ui) -> Option<String> {
    let head = header(h);
    h.root()
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| {
            let r = n.rect();
            r.top() > head.bottom() && r.left() < head.left() + 400.0
        })
        .and_then(|n| n.value())
}

fn start_rename_by_double_click(h: &mut Ui, name: &str) {
    let at = layer_row(h, name);
    double_click_at(h, at);
    assert_eq!(
        rename_field_value(h).as_deref(),
        Some(name),
        "double-click did not open the rename field"
    );
}

/// Select-all then type, so the test does not depend on the initial selection.
fn replace_text(h: &mut Ui, text: &str) {
    press(h, Modifiers::COMMAND, Key::A);
    send(h, Event::Text(text.to_owned()));
    settle(h);
}

fn undo(h: &mut Ui) {
    key(h, Modifiers::COMMAND, Key::Z);
}

fn redo(h: &mut Ui) {
    key(h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
}

#[test]
fn rename_commits_on_enter_and_click_away_and_esc_cancels() {
    let mut h = comp_with_solid();
    let layer = h.state().layers()[0].clone();

    // Double-click, type, Enter.
    start_rename_by_double_click(&mut h, "Dark Gray Solid 1");
    replace_text(&mut h, "Hero");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), "Hero");
    assert!(
        rename_field_value(&h).is_none(),
        "field still open after Enter"
    );

    // Double-click, type, click elsewhere (an empty part of the timeline graph).
    start_rename_by_double_click(&mut h, "Hero");
    replace_text(&mut h, "Hero 2");
    let head = header(&h);
    click_at(&mut h, pos2(head.left() + 900.0, head.bottom() + 80.0));
    assert_eq!(
        h.state().name(&layer),
        "Hero 2",
        "click away did not commit"
    );

    // Double-click, type, Esc: unchanged.
    start_rename_by_double_click(&mut h, "Hero 2");
    replace_text(&mut h, "Discarded");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(h.state().name(&layer), "Hero 2", "Esc did not cancel");
    assert!(rename_field_value(&h).is_none());

    // Each committed rename is one undo step.
    undo(&mut h);
    assert_eq!(h.state().name(&layer), "Hero");
    undo(&mut h);
    assert_eq!(h.state().name(&layer), "Dark Gray Solid 1");
    redo(&mut h);
    redo(&mut h);
    assert_eq!(h.state().name(&layer), "Hero 2");
}

#[test]
fn rename_field_starts_with_the_whole_name_selected() {
    // AE selects the whole name when the field opens: typing replaces it.
    let mut h = comp_with_solid();
    let layer = h.state().layers()[0].clone();
    start_rename_by_double_click(&mut h, "Dark Gray Solid 1");
    type_text(&mut h, "Hero");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), "Hero");
}

#[test]
fn enter_on_a_selected_timeline_layer_starts_rename() {
    // AE: select a layer in the timeline, press Enter (Return) to rename it.
    let mut h = comp_with_solid();
    let at = layer_row(&h, "Dark Gray Solid 1");
    click_at(&mut h, at);
    assert_eq!(h.state().selected_layers(), h.state().layers());
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(
        rename_field_value(&h).as_deref(),
        Some("Dark Gray Solid 1"),
        "Enter on the selected layer did not open the rename field"
    );
}

#[test]
fn rename_with_ime_spaces_emoji_and_long_names() {
    let mut h = comp_with_solid();
    let layer = h.state().layers()[0].clone();

    // Chinese through a Pinyin composition: marked text, then the commit.
    start_rename_by_double_click(&mut h, "Dark Gray Solid 1");
    press(&mut h, Modifiers::COMMAND, Key::A);
    settle(&mut h);
    ime_compose(&mut h, &["b", "bei", "bei j", "bei jing"], "背景");
    // The field shows the committed text, not the marked text.
    assert_eq!(rename_field_value(&h).as_deref(), Some("背景"));
    // A second composition appended after the first.
    ime_compose(&mut h, &["c", "ce", "ceng"], "层");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), "背景层");

    // Spaces inside, leading and trailing.
    for name in ["  spaced  name  ", "Comp 背景 ✨ 🎬👩‍👩‍👧", "a"] {
        let current = h.state().name(&layer);
        start_rename_by_double_click(&mut h, &current);
        replace_text(&mut h, name);
        key(&mut h, Modifiers::NONE, Key::Enter);
        assert_eq!(h.state().name(&layer), name, "rename to {name:?}");
    }

    // A whitespace-only name is refused (the layer keeps its name).
    start_rename_by_double_click(&mut h, "a");
    replace_text(&mut h, "   ");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), "a");

    // A very long name (2 000 characters, mixed scripts).
    let long: String = "长名字 Long name ".repeat(125);
    start_rename_by_double_click(&mut h, "a");
    replace_text(&mut h, &long);
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), long);
    // The row still paints (clipped) and the app keeps running.
    settle(&mut h);
    assert!(
        !find_texts(&h, |t| t.starts_with("长名字")).is_empty(),
        "long name not painted"
    );

    // All five renames undo back to the original name.
    for _ in 0..5 {
        undo(&mut h);
    }
    assert_eq!(h.state().name(&layer), "Dark Gray Solid 1");
}

#[test]
fn space_in_a_text_field_types_and_does_not_play() {
    let mut h = comp_with_solid();
    start_rename_by_double_click(&mut h, "Dark Gray Solid 1");
    replace_text(&mut h, "A");
    press(&mut h, Modifiers::NONE, Key::Space);
    send(&mut h, Event::Text(" ".into()));
    send(&mut h, Event::Text("B".into()));
    settle(&mut h);
    assert!(
        !h.state().is_playing(),
        "Space in the rename field started playback"
    );
    assert_eq!(rename_field_value(&h).as_deref(), Some("A B"));
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().layer_names(), ["A B"]);
}

#[test]
fn space_with_nothing_focused_plays_and_stops() {
    let mut h = comp_with_solid();
    assert!(!h.ctx.egui_wants_keyboard_input());
    press(&mut h, Modifiers::NONE, Key::Space);
    frame(&mut h);
    assert!(h.state().is_playing(), "Space did not start playback");
    // Playback advances the time while frames run: wait for it (a slow frame
    // on a busy machine must not fail this), with a limit for a stuck clock.
    let started = std::time::Instant::now();
    while h.state().now() == Time::ZERO {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "time did not advance in 10 s of playback"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        frame(&mut h);
    }
    assert!(h.state().is_playing(), "playback stopped by itself");
    press(&mut h, Modifiers::NONE, Key::Space);
    frame(&mut h);
    assert!(
        !h.state().is_playing(),
        "second Space did not stop playback"
    );
    // One play action per press (not handled twice).
    let plays = h
        .state()
        .dispatched
        .iter()
        .filter(|i| i.id == "motion.play")
        .count();
    assert_eq!(plays, 2);
}

#[test]
fn space_on_a_focused_button_presses_it_and_does_not_play() {
    let mut h = comp_with_solid();
    // The Render Queue's "Remove" button gets keyboard focus (Tab), then Space.
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    assert_eq!(h.state().render_queue().lock().items().len(), 1);
    queue_control(&h, 1, "Remove").expect("Remove").focus();
    settle(&mut h);
    press(&mut h, Modifiers::NONE, Key::Space);
    settle(&mut h);
    assert!(
        !h.state().is_playing(),
        "Space on a focused button started playback"
    );
    assert!(
        h.state().render_queue().lock().items().is_empty(),
        "Space did not press the focused button"
    );
}

/// Reveal Position (P) and click its stopwatch; returns the layer.
fn position_stopwatch(h: &mut Ui) -> PrimPath {
    let layer = h.state().layers()[0].clone();
    key(h, Modifiers::NONE, Key::P);
    let label = timeline_text(h, "Position")
        .unwrap_or_else(|| panic!("P did not reveal Position: {:?}", painted_texts(h)));
    // The stopwatch is drawn just left of the property name.
    click_at(h, pos2(label.left() - 12.0, label.center().y));
    layer
}

fn key_times(h: &Ui, layer: &PrimPath) -> Vec<Time> {
    bae_motion::query::attr_spec(h.state().stage(), layer, POSITION)
        .map(|a| a.keyframes.keys().copied().collect())
        .unwrap_or_default()
}

fn frames(h: &mut Ui, n: usize) {
    for _ in 0..n {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    settle(h);
}

#[test]
fn stopwatch_option_shift_p_and_j_k() {
    let mut h = comp_with_solid();
    let layer = position_stopwatch(&mut h);
    assert_eq!(
        key_times(&h, &layer),
        [Time::ZERO],
        "stopwatch made no keyframe at 0"
    );

    // 15 frames on, Option+Shift+P adds a keyframe at the current time.
    frames(&mut h, 15);
    let t15 = h.state().now();
    assert!(t15 > Time::ZERO);
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(key_times(&h, &layer), [Time::ZERO, t15]);
    // And at 45 frames.
    frames(&mut h, 30);
    let t45 = h.state().now();
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(key_times(&h, &layer), [Time::ZERO, t15, t45]);
    // Option+Shift+P on an existing keyframe removes it (AE toggles).
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(key_times(&h, &layer), [Time::ZERO, t15]);
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);

    // J / K jump between the keyframes of the selected layer.
    key(&mut h, Modifiers::NONE, Key::Home);
    assert_eq!(h.state().now(), Time::ZERO);
    key(&mut h, Modifiers::NONE, Key::K);
    assert_eq!(h.state().now(), t15);
    key(&mut h, Modifiers::NONE, Key::K);
    assert_eq!(h.state().now(), t45);
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(h.state().now(), t15);
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(h.state().now(), Time::ZERO);
    // J at the first keyframe stays.
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(h.state().now(), Time::ZERO);

    // Undo the keyframe edits one by one, then redo them.
    let full = key_times(&h, &layer);
    undo(&mut h); // re-add at 45
    undo(&mut h); // remove at 45
    assert_eq!(key_times(&h, &layer), [Time::ZERO, t15, t45]);
    undo(&mut h); // add at 45
    undo(&mut h); // add at 15
    undo(&mut h); // stopwatch
    assert!(key_times(&h, &layer).is_empty(), "undo left keyframes");
    for _ in 0..5 {
        redo(&mut h);
    }
    assert_eq!(key_times(&h, &layer), full);
}

#[test]
fn stopwatch_off_removes_all_keyframes_and_undo_restores_them() {
    let mut h = comp_with_solid();
    let layer = position_stopwatch(&mut h);
    frames(&mut h, 10);
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    assert_eq!(key_times(&h, &layer).len(), 2);
    // Clicking the stopwatch again turns animation off (AE: keyframes deleted).
    let label = timeline_text(&h, "Position").unwrap();
    click_at(&mut h, pos2(label.left() - 12.0, label.center().y));
    assert!(key_times(&h, &layer).is_empty());
    undo(&mut h);
    assert_eq!(key_times(&h, &layer).len(), 2);
}

#[test]
fn selection_by_click_and_shift_click() {
    let mut h = gui();
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.text",
        ],
    );
    let names = h.state().layer_names();
    assert_eq!(names.len(), 3, "{names:?}");
    let first = layer_row(&h, &names[0]);
    click_at(&mut h, first);
    assert_eq!(h.state().selected_layers(), [h.state().layers()[0].clone()]);
    let third = layer_row(&h, &names[2]);
    click_at_mods(&mut h, third, Modifiers::SHIFT);
    assert_eq!(h.state().selected_layers().len(), 3, "Shift+click range");
    // Cmd+click removes one layer from the selection, and adds it back.
    let second = layer_row(&h, &names[1]);
    click_at_mods(&mut h, second, Modifiers::COMMAND);
    assert_eq!(
        h.state().selected_layers().len(),
        2,
        "Cmd+click toggles off"
    );
    click_at_mods(&mut h, second, Modifiers::COMMAND);
    assert_eq!(h.state().selected_layers().len(), 3, "Cmd+click toggles on");
    // Cmd+Shift+A deselects all.
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    assert!(h.state().selected_layers().is_empty());
    // Cmd+A selects all.
    key(&mut h, Modifiers::COMMAND, Key::A);
    assert_eq!(h.state().selected_layers().len(), 3);
}

#[test]
fn project_panel_enter_renames_the_selected_item() {
    let mut h = comp_with_solid();
    let comp = h.state().active_comp().unwrap();
    let row = find_text(&h, "Comp 1").expect("comp row in the Project panel");
    click_at(&mut h, row.center());
    key(&mut h, Modifiers::NONE, Key::Enter);
    replace_text(&mut h, "片头 Intro");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&comp), "片头 Intro");
}

// ---- Retest of #106 / #107 (dev 763eaad): edge cases from a user's side ----

/// Click a layer row, then Enter: the rename field opens with the whole name
/// selected; a Pinyin composition replaces it.
#[test]
fn enter_rename_then_ime_replaces_the_selected_name() {
    let mut h = comp_with_solid();
    let layer = h.state().layers()[0].clone();
    let at = layer_row(&h, "Dark Gray Solid 1");
    click_at(&mut h, at);
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(rename_field_value(&h).as_deref(), Some("Dark Gray Solid 1"));
    ime_compose(&mut h, &["bei", "bei jing"], "背景");
    assert_eq!(
        rename_field_value(&h).as_deref(),
        Some("背景"),
        "IME did not replace the selection"
    );
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), "背景");
    assert!(rename_field_value(&h).is_none());
    // The Enter that committed did not open the field again.
    settle(&mut h);
    assert!(
        rename_field_value(&h).is_none(),
        "Enter re-opened the rename field"
    );
    // Enter again renames again (selection kept), Esc cancels.
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(rename_field_value(&h).as_deref(), Some("背景"));
    type_text(&mut h, "x");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(h.state().name(&layer), "背景");
}

#[test]
fn enter_renames_the_topmost_selected_layer() {
    let mut h = gui();
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.solid",
        ],
    );
    let layers = h.state().layers();
    // Select the bottom and the top layer (Cmd+click), in that order.
    let bottom = layer_row(&h, &h.state().name(&layers[2]));
    let top = layer_row(&h, &h.state().name(&layers[0]));
    click_at(&mut h, bottom);
    click_at_mods(&mut h, top, Modifiers::COMMAND);
    assert_eq!(h.state().selected_layers().len(), 2);
    key(&mut h, Modifiers::NONE, Key::Enter);
    let top_name = h.state().name(&layers[0]);
    assert_eq!(
        rename_field_value(&h),
        Some(top_name),
        "Enter did not rename the topmost selected layer"
    );
    replace_text(&mut h, "顶层");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layers[0]), "顶层");
    assert_ne!(h.state().name(&layers[2]), "顶层");
}

#[test]
fn enter_on_a_focused_button_does_not_start_a_rename() {
    let mut h = comp_with_solid();
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    assert_eq!(h.state().render_queue().lock().items().len(), 1);
    h.state().session().lock().selected_layers = h.state().layers();
    queue_control(&h, 1, "Remove").expect("Remove").focus();
    settle(&mut h);
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(
        h.state().render_queue().lock().items().is_empty(),
        "Enter did not press the button"
    );
    to_front(&mut h, "timeline");
    assert!(
        rename_field_value(&h).is_none(),
        "Enter on a button also started a rename"
    );
}

#[test]
fn double_click_rename_also_selects_the_name_for_ime() {
    let mut h = comp_with_solid();
    let layer = h.state().layers()[0].clone();
    start_rename_by_double_click(&mut h, "Dark Gray Solid 1");
    ime_compose(&mut h, &["wen", "wen zi"], "文字");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&layer), "文字");
}

/// Five layers; returns their timeline row positions top to bottom.
fn five_layers(h: &mut Ui) -> Vec<Pos2> {
    comp_with(
        h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.solid",
            "motion.layer.new.null",
            "motion.layer.new.solid",
        ],
    );
    let names = h.state().layer_names();
    names.iter().map(|n| layer_row(h, n)).collect()
}

#[test]
fn shift_click_range_grows_shrinks_and_goes_upward() {
    let mut h = gui();
    let rows = five_layers(&mut h);
    let layers = h.state().layers();
    let sel = |h: &Ui| {
        let mut s: Vec<usize> = h
            .state()
            .selected_layers()
            .iter()
            .map(|l| layers.iter().position(|x| x == l).unwrap())
            .collect();
        s.sort();
        s
    };
    click_at(&mut h, rows[1]);
    click_at_mods(&mut h, rows[3], Modifiers::SHIFT);
    assert_eq!(sel(&h), [1, 2, 3], "range down");
    // A second Shift+click re-ranges from the same anchor (shrinks).
    click_at_mods(&mut h, rows[2], Modifiers::SHIFT);
    assert_eq!(sel(&h), [1, 2], "range shrinks from the anchor");
    // Upward from the anchor.
    click_at_mods(&mut h, rows[0], Modifiers::SHIFT);
    assert_eq!(sel(&h), [0, 1], "range up");
    // A plain click moves the anchor.
    click_at(&mut h, rows[4]);
    click_at_mods(&mut h, rows[2], Modifiers::SHIFT);
    assert_eq!(sel(&h), [2, 3, 4], "new anchor");
    // Shift+click with nothing selected selects just that layer.
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    click_at_mods(&mut h, rows[3], Modifiers::SHIFT);
    assert_eq!(sel(&h), [3], "Shift+click with no anchor");
}

/// Locks `layer` as the timeline's lock switch does (`attr.set locked`).
fn lock(h: &mut Ui, layer: &PrimPath) {
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        serde_json::json!({"path": layer, "attr": "locked", "value": {"type": "bool", "value": true}}),
    ));
    settle(h);
}

#[test]
fn locked_layers_are_skipped_by_clicks_ranges_select_all_and_delete() {
    // docs/spec/ae/project-comp-layers.md: a locked layer cannot be selected by
    // Timeline clicks, Select All skips locked layers, locked layers cannot be
    // deleted.
    let mut h = gui();
    let rows = five_layers(&mut h);
    let layers = h.state().layers();
    lock(&mut h, &layers[2]);
    let locked = layers[2].clone();
    let mut wrong = Vec::new();
    click_at(&mut h, rows[2]);
    if h.state().selected_layers().contains(&locked) {
        wrong.push("a click selects the locked layer");
    }
    click_at(&mut h, rows[1]);
    click_at_mods(&mut h, rows[3], Modifiers::SHIFT);
    if h.state().selected_layers().contains(&locked) {
        wrong.push("a Shift+click range includes the locked layer");
    }
    key(&mut h, Modifiers::COMMAND, Key::A);
    if h.state().selected_layers().contains(&locked) {
        wrong.push("Select All (Cmd+A) includes the locked layer");
    }
    // Rename through Enter.
    h.state().session().lock().selected_layers = vec![locked.clone()];
    key(&mut h, Modifiers::NONE, Key::Enter);
    if rename_field_value(&h).is_some() {
        wrong.push("Enter opens a rename field on the locked layer");
        key(&mut h, Modifiers::NONE, Key::Escape);
    }
    // Delete with the timeline focused.
    let head = header(&h);
    click_at(&mut h, pos2(head.left() + 800.0, head.bottom() + 150.0));
    h.state().session().lock().selected_layers = vec![locked.clone()];
    key(&mut h, Modifiers::NONE, Key::Delete);
    if h.state().stage().prim(&locked).is_none() {
        wrong.push("Delete removes the locked layer");
        key(&mut h, Modifiers::COMMAND, Key::Z);
    }
    // Arrange / duplicate act on it.
    h.state().session().lock().selected_layers = vec![locked.clone()];
    key(
        &mut h,
        Modifiers::COMMAND | Modifiers::SHIFT,
        Key::CloseBracket,
    );
    if h.state().layers()[0] == locked {
        wrong.push("Bring to Front moves the locked layer");
    }
    assert!(wrong.is_empty(), "locked layer: {wrong:?}");
}

/// Home / End / PageUp / PageDown / Shift+PageDown with a timeline layer just
/// clicked (the timeline focused) move the current time, as in AE (#468) —
/// also when Windows reports them with the numpad's scan codes (NumLock off,
/// or sent by automation): numpad 7 / 1 / 9 / 3, which are also the timeline's
/// Go to Comp Marker keys.
#[test]
fn transport_keys_move_the_playhead_after_clicking_a_layer() {
    let mut h = comp_with_solid();
    // As the report: click the time ruler first, then a layer.
    let head = header(&h);
    let tick = find_texts(&h, |t| t == "2s")
        .into_iter()
        .find(|r| r.left() > head.right())
        .unwrap_or_else(|| panic!("no ruler label in {:?}", painted_texts(&h)));
    click_at(&mut h, tick.center());
    settle(&mut h);
    let start = h.state().now();
    assert!(start > Time::ZERO, "the ruler click set the time");
    let row = layer_row(&h, "Dark Gray Solid 1");
    click_at(&mut h, row);
    settle(&mut h);
    for numpad in [false, true] {
        let step = |h: &mut Ui, mods: Modifiers, k: Key| {
            let physical = match (numpad, k) {
                (true, Key::Home) => Key::Num7,
                (true, Key::End) => Key::Num1,
                (true, Key::PageUp) => Key::Num9,
                (true, Key::PageDown) => Key::Num3,
                _ => k,
            };
            let e = |pressed| Event::Key {
                key: k,
                physical_key: Some(physical),
                pressed,
                repeat: false,
                modifiers: mods,
            };
            for ev in [
                Event::ModifiersChanged(mods),
                e(true),
                e(false),
                Event::ModifiersChanged(Modifiers::NONE),
            ] {
                send(h, ev);
            }
            settle(h);
            h.state().now()
        };
        assert_eq!(
            step(&mut h, Modifiers::NONE, Key::Home),
            Time::ZERO,
            "Home ({numpad})"
        );
        let one = step(&mut h, Modifiers::NONE, Key::PageDown);
        assert!(
            one > Time::ZERO,
            "PageDown did not step from {start:?} ({numpad})"
        );
        let eleven = step(&mut h, Modifiers::SHIFT, Key::PageDown);
        assert_eq!(
            eleven.0,
            one.0 * 11,
            "Shift+PageDown steps 10 frames ({numpad})"
        );
        let ten = step(&mut h, Modifiers::NONE, Key::PageUp);
        assert_eq!(ten.0, one.0 * 10, "PageUp steps back ({numpad})");
        let end = step(&mut h, Modifiers::NONE, Key::End);
        assert!(end > ten, "End goes to the last frame ({numpad})");
    }
    let comp = h.state().active_comp().expect("comp");
    assert!(
        bae_motion::markers::markers_of(h.state().stage(), &comp).is_empty(),
        "Shift+PageDown set no comp marker"
    );
}

fn right_click(h: &mut Ui, at: Pos2) {
    pause(h);
    send(h, Event::PointerMoved(at));
    for pressed in [true, false] {
        send(
            h,
            Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        );
    }
    settle(h);
}

/// Right-click in the timeline: under the layers AE's New menu, on the layer
/// number and label columns the layer menu (#466).
#[test]
fn timeline_right_click_menus() {
    let mut h = comp_with_solid();
    let row = layer_row(&h, "Dark Gray Solid 1");
    right_click(&mut h, pos2(row.x, row.y + 80.0));
    let texts = painted_texts(&h);
    assert!(
        texts.iter().any(|t| t == "New"),
        "New menu under the layers: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "Pre-compose..."),
        "no layer menu: {texts:?}"
    );
    send(
        &mut h,
        Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    );
    settle(&mut h);
    let number = find_texts(&h, |t| t == "1")
        .into_iter()
        .find(|r| (r.center().y - row.y).abs() < 6.0)
        .expect("layer number");
    right_click(&mut h, number.center());
    let texts = painted_texts(&h);
    assert!(
        texts.iter().any(|t| t == "Pre-compose..."),
        "layer menu on #: {texts:?}"
    );
}
