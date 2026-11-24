//! The Keyboard Shortcuts dialog used like AE's (docs/reference/ae/mac/ui/keyboard-shortcuts.json):
//! opened with Cmd/Ctrl+Alt+'; search; the Commands scope; a shortcut recorded by
//! clicking the Shortcut column, a conflict message, Undo, Clear; OK keeps the
//! change (also after a restart), Cancel drops it.

use bae_ui::{Chord, Keymap, WorkbenchState};
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Rect};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

use super::*;

const SETTINGS: &str = "Composition Settings...";

fn window(h: &Ui) -> Option<Rect> {
    let title = bae_ui::tr("Keyboard Shortcuts");
    h.query_all_by_role(Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some(title))
        .map(|n| n.rect())
}

/// The row of a command in the tree (its label is the name, its value the shortcuts).
fn row(h: &Ui, name: &str) -> Option<(Rect, String)> {
    let w = window(h)?;
    h.query_all_by_label(name)
        .find(|n| w.contains_rect(n.rect()))
        .map(|n| (n.rect(), n.accesskit_node().value().unwrap_or_default()))
}

fn click_label(h: &mut Ui, label: &str) {
    let r = h
        .query_all_by_label(label)
        .map(|n| n.rect())
        .find(|r| window(h).is_some_and(|w| w.contains_rect(*r)))
        .unwrap_or_else(|| panic!("no {label} in the dialog"));
    click_at(h, r.center());
    settle(h);
}

/// Click the empty part of a row's Shortcut column.
fn click_shortcut_cell(h: &mut Ui, name: &str) {
    let (r, _) = row(h, name).unwrap_or_else(|| panic!("no row {name}"));
    click_at(h, egui::pos2(r.right() - 30.0, r.center().y));
    settle(h);
}

fn search(h: &mut Ui, text: &str) {
    type_text(h, text);
    settle(h);
}

fn chord(s: &str) -> Chord {
    Chord::parse(s).unwrap()
}

fn action_of(km: &Keymap, c: &str) -> Option<String> {
    km.lookup(&chord(c), None).map(|b| b.action.clone())
}

#[test]
fn record_a_shortcut_resolve_the_conflict_and_keep_it_with_ok() {
    let mut h = gui_sized(Gui::new(), egui::vec2(1600.0, 1000.0));
    let km = h.state().workspace.keymap().clone();
    let settings = action_of(&km, "Cmd+K").expect("Composition Settings on Cmd/Ctrl+K");
    let new_comp = action_of(&km, "Cmd+N").expect("New Composition on Cmd/Ctrl+N");

    // Opened with its shortcut; the search finds the command.
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    settle(&mut h);
    assert!(window(&h).is_some(), "Cmd/Ctrl+Alt+' opened no dialog");
    search(&mut h, "composition settings");
    let (_, shown) = row(&h, SETTINGS).expect("the search shows Composition Settings");
    assert!(shown.ends_with('K'), "{shown}");
    assert!(
        row(&h, "New Composition...").is_none(),
        "the search filters"
    );

    // Shortcut column: first click focuses its shortcut, the second adds an
    // empty one; Cmd/Ctrl+N is New Composition's: it moves, with a message.
    click_shortcut_cell(&mut h, SETTINGS);
    click_shortcut_cell(&mut h, SETTINGS);
    key(&mut h, Modifiers::COMMAND, Key::N);
    settle(&mut h);
    let message = bae_ui::tr(
        "The shortcut: {chord} is already in use by another Application command: {command}. That command will no longer have a shortcut.",
    );
    let head = message.split("{chord}").next().unwrap();
    assert!(
        !find_texts(&h, |t| t.starts_with(head)).is_empty(),
        "no conflict message: {:?}",
        painted_texts(&h)
    );
    assert!(
        h.query_all_by_label("New Composition...")
            .any(|n| n.accesskit_node().role() == Role::Link),
        "the message links to New Composition"
    );
    let (_, shown) = row(&h, SETTINGS).unwrap();
    assert!(shown.contains('K') && shown.contains('N'), "{shown}");
    // Not live before OK.
    assert_eq!(
        action_of(h.state().workspace.keymap(), "Cmd+N").as_deref(),
        Some(new_comp.as_str())
    );

    // Undo takes it back; record again and OK.
    click_label(&mut h, bae_ui::tr("Undo"));
    let (_, shown) = row(&h, SETTINGS).expect("the row is still shown");
    assert!(!shown.contains('N'), "{shown}");
    click_shortcut_cell(&mut h, SETTINGS);
    key(&mut h, Modifiers::COMMAND, Key::N);
    settle(&mut h);
    click_label(&mut h, bae_ui::tr("OK"));
    assert!(window(&h).is_none(), "OK closes the dialog");
    let km = h.state().workspace.keymap().clone();
    assert_eq!(action_of(&km, "Cmd+N").as_deref(), Some(settings.as_str()));
    assert_eq!(action_of(&km, "Cmd+K").as_deref(), Some(settings.as_str()));

    // Saved with the workspace state: still there after a restart.
    let state = h.state_mut().workspace.state();
    let saved: WorkbenchState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    let mut app = build_app();
    let restarted = crate::ui::Workspace::new(&mut app, saved);
    assert_eq!(
        action_of(restarted.keymap(), "Cmd+N").as_deref(),
        Some(settings.as_str())
    );

    // Reopened: the Custom preset. Clear removes the focused shortcut; Cancel
    // drops it.
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    settle(&mut h);
    assert!(
        h.query_all_by_role(Role::ComboBox)
            .any(|n| n.accesskit_node().value().as_deref() == Some(bae_ui::tr("Custom.txt"))),
        "the preset is Custom after a change"
    );
    search(&mut h, "composition settings");
    click_shortcut_cell(&mut h, SETTINGS);
    click_label(&mut h, bae_ui::tr("Clear"));
    let (_, shown) = row(&h, SETTINGS).unwrap();
    assert_eq!(shown.matches('+').count(), 1, "one shortcut left: {shown}");
    click_label(&mut h, bae_ui::tr("Cancel"));
    assert!(window(&h).is_none());
    let km = h.state().workspace.keymap().clone();
    assert_eq!(action_of(&km, "Cmd+K").as_deref(), Some(settings.as_str()));
    assert_eq!(action_of(&km, "Cmd+N").as_deref(), Some(settings.as_str()));
}

/// Commands: a panel scope shows that panel's group of the tree.
#[test]
fn the_commands_menu_shows_a_panel_scope() {
    let mut h = gui_sized(Gui::new(), egui::vec2(1600.0, 1000.0));
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    settle(&mut h);
    let combo = h
        .query_all_by_role(Role::ComboBox)
        .find(|n| n.accesskit_node().label().as_deref() == Some(bae_ui::tr("Commands:")))
        .map(|n| n.rect())
        .expect("the Commands menu");
    click_at(&mut h, combo.center());
    settle(&mut h);
    let scopes: Vec<String> = h
        .query_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| l.ends_with(" Panel") || l.ends_with(" Panels"))
        .collect();
    assert!(scopes.len() >= 11, "AE's panel scopes: {scopes:?}");
    click_label(&mut h, "Timeline Panel");
    let group = h
        .query_all_by_label("Timeline Panel")
        .find(|n| n.accesskit_node().role() == Role::Button)
        .expect("the Timeline Panel group");
    assert_eq!(
        group.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the group is open"
    );
}

/// The keyboard: clicking ⌘ (Ctrl on Windows) holds it, clicking K lists its
/// combinations under "Key:", a command dragged onto a key gets that key.
#[test]
fn the_keyboard_holds_modifiers_lists_keys_and_takes_dropped_commands() {
    let mut h = gui_sized(Gui::new(), egui::vec2(1600.0, 1000.0));
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    settle(&mut h);
    let w = window(&h).expect("dialog");
    let cap = |h: &Ui, label: &str| {
        h.query_all_by_label(label)
            .find(|n| n.accesskit_node().role() == Role::Button && w.contains_rect(n.rect()))
            .map(|n| n.rect())
            .unwrap_or_else(|| panic!("no key {label}"))
    };
    let command = if cfg!(target_os = "macos") {
        "\u{2318} Cmd"
    } else {
        "Ctrl"
    };
    let at = cap(&h, command).center();
    click_at(&mut h, at);
    settle(&mut h);
    let held = h
        .query_all_by_label(command)
        .find(|n| n.accesskit_node().role() == Role::Button)
        .unwrap();
    assert_eq!(
        held.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the modifier cap stays down"
    );
    // ⌘K / Ctrl+K: Composition Settings is written on K.
    let k = h
        .query_all_by_label("K")
        .find(|n| n.accesskit_node().role() == Role::Button && w.contains_rect(n.rect()))
        .unwrap();
    assert_eq!(k.accesskit_node().value().as_deref(), Some(SETTINGS));
    let k = k.rect();
    click_at(&mut h, k.center());
    settle(&mut h);
    assert!(
        find_text(&h, "Key: K (multiple commands assigned)").is_some(),
        "{:?}",
        painted_texts(&h)
    );
    // Release ⌘, then drag a tool onto F5.
    let at = cap(&h, command).center();
    click_at(&mut h, at);
    settle(&mut h);
    let (from, _) = row(&h, "Hand Tool").expect("the Hand Tool row");
    let to = cap(&h, "F5").center();
    let from = egui::pos2(from.left() + 40.0, from.center().y);
    h.input_mut().events.extend([
        Event::PointerMoved(from),
        Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]);
    frame(&mut h);
    for i in 1..=12 {
        send(
            &mut h,
            Event::PointerMoved(from + (to - from) * (i as f32 / 12.0)),
        );
    }
    send(
        &mut h,
        Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    );
    settle(&mut h);
    let f5 = h
        .query_all_by_label("F5")
        .find(|n| n.accesskit_node().role() == Role::Button)
        .unwrap();
    assert_eq!(f5.accesskit_node().value().as_deref(), Some("Hand Tool"));
    let (_, shown) = row(&h, "Hand Tool").unwrap();
    assert!(shown.ends_with("F5"), "{shown}");
}

/// Presets like AE's: Save As names a new preset (a file in the presets folder
/// on OK), Copy To Clipboard says so, Delete asks first.
#[test]
fn presets_save_as_copy_and_delete() {
    let dir = std::env::temp_dir().join(format!("bae-shortcut-presets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut h = gui_sized(Gui::new(), egui::vec2(1600.0, 1000.0));
    h.state_mut()
        .workspace
        .set_shortcut_presets_dir(Some(dir.clone()));
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    settle(&mut h);
    let preset = |h: &Ui| {
        h.query_all_by_role(Role::ComboBox)
            .find(|n| n.accesskit_node().label().as_deref() == Some("Preset:"))
            .and_then(|n| n.accesskit_node().value())
            .unwrap_or_default()
    };
    assert_eq!(preset(&h), "Default.txt");

    click_label(&mut h, "Copy To Clipboard");
    assert!(find_text(&h, "Copied to clipboard").is_some(), "no notice");

    // Save As: "Create Preset" with the name selected; typing replaces it.
    click_label(&mut h, "Save As...");
    assert!(
        find_text(&h, "Create Preset").is_some(),
        "{:?}",
        painted_texts(&h)
    );
    type_text(&mut h, "Mine");
    let save = h
        .query_all_by_label("Save")
        .find(|n| n.accesskit_node().role() == Role::Button)
        .map(|n| n.rect())
        .expect("Save");
    click_at(&mut h, save.center());
    settle(&mut h);
    assert_eq!(preset(&h), "Mine.txt");
    assert!(!dir.exists(), "written on OK only");
    click_label(&mut h, "OK");
    assert!(dir.join("Mine.txt").is_file(), "the preset file");

    // Delete asks, then the built-in preset is selected; OK removes the file.
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Quote);
    settle(&mut h);
    assert_eq!(preset(&h), "Mine.txt");
    // The preset row's Delete (the Delete key cap has the same name).
    let delete = h
        .query_all_by_label("Delete")
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    click_at(&mut h, delete.center());
    settle(&mut h);
    assert!(
        find_texts(&h, |t| t.starts_with("Are you sure you want to delete")).len() == 1,
        "{:?}",
        painted_texts(&h)
    );
    // The question's own window is the smaller one.
    let question = h
        .query_all_by_role(Role::Window)
        .map(|n| n.rect())
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap();
    let confirm = h
        .query_all_by_label("Delete")
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .map(|n| n.rect())
        .find(|r| question.contains_rect(*r))
        .expect("Delete in the question");
    click_at(&mut h, confirm.center());
    settle(&mut h);
    assert_eq!(preset(&h), "Default.txt");
    click_label(&mut h, "OK");
    assert!(!dir.join("Mine.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
