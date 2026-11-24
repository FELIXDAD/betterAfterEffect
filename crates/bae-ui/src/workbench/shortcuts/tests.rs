use std::sync::Arc;

use bae_app::{Owner, Registry, Resources};
use bae_model::{AuthorId, Stage};
use egui::{Event, Key, Modifiers, RawInput, Rect, vec2};

use super::*;
use crate::action::{ActionInfo, Invocation, ids};
use crate::dock::Node;
use crate::menu::MenuItemSpec;
use crate::workbench::{WorkbenchContext, WorkbenchState};
use crate::{PanelKind, WorkspacePreset};

struct Empty;
impl crate::Panel for Empty {
    fn ui(&mut self, _ui: &mut Ui, _cx: &mut crate::PanelContext<'_>) {}
}

fn workbench() -> Workbench {
    let owner = Owner::module("test");
    let kinds = Arc::new(Registry::<PanelKind>::new());
    for k in ["timeline", "project"] {
        kinds
            .register(owner.clone(), k, PanelKind::new(k, |_| Box::new(Empty)))
            .unwrap();
    }
    let presets = Arc::new(Registry::<WorkspacePreset>::new());
    presets
        .register(
            owner.clone(),
            "one",
            WorkspacePreset::new("One", "km", Node::tabs(&["timeline"])),
        )
        .unwrap();
    let keymaps = Arc::new(Registry::<Keymap>::new());
    keymaps
        .register(
            owner.clone(),
            "km",
            Keymap::new(
                "KM",
                vec![
                    Binding::new("Cmd+K", "comp.settings"),
                    Binding::new("Cmd+N", "comp.new"),
                    Binding::new("V", "motion.tool.selection"),
                    Binding::new("U", "layer.reveal_keyframes"),
                    Binding::new("J", "timeline.prev_key").when("timeline"),
                    Binding::new("B", "timeline.work_start").when("timeline"),
                ],
            ),
        )
        .unwrap();
    let actions = Arc::new(Registry::<ActionInfo>::new());
    for (id, title, panel) in [
        ("comp.settings", "Composition Settings", None),
        ("comp.new", "New Composition", None),
        ("motion.tool.selection", "Selection", None),
        ("layer.reveal_keyframes", "Reveal Keyframes", None),
        (
            "timeline.prev_key",
            "Go to Previous Keyframe",
            Some("timeline"),
        ),
        (
            "timeline.work_start",
            "Set Work Area Start",
            Some("timeline"),
        ),
        (ids::UNDO, "Undo", None),
    ] {
        let mut info = ActionInfo::new(title);
        if let Some(p) = panel {
            info = info.for_panel(p);
        }
        actions.register(owner.clone(), id, info).unwrap();
    }
    let menus = Arc::new(Registry::<MenuItemSpec>::new());
    for (id, path, title, action) in [
        (
            "m.new",
            &["Composition"][..],
            "New Composition...",
            "comp.new",
        ),
        (
            "m.settings",
            &["Composition"][..],
            "Composition Settings...",
            "comp.settings",
        ),
        ("m.undo", &["Edit"][..], "Undo", ids::UNDO),
        (
            "m.reveal",
            &["Animation", "Reveal"][..],
            "Reveal Keyframes",
            "layer.reveal_keyframes",
        ),
    ] {
        menus
            .register(owner.clone(), id, MenuItemSpec::new(path, title, action))
            .unwrap();
    }
    Workbench::with_registries(kinds, presets, keymaps, actions, WorkbenchState::default())
        .with_menus(menus)
}

fn run(wb: &mut Workbench, events: Vec<Event>) {
    let ctx = crate::testing::ctx_with_fonts();
    run_in(&ctx, wb, events);
}

fn run_in(ctx: &egui::Context, wb: &mut Workbench, events: Vec<Event>) -> Vec<Invocation> {
    let stage = Stage::new();
    let author = AuthorId::system();
    let resources = Resources::new();
    let cx = WorkbenchContext {
        stage: &stage,
        author: &author,
        resources: &resources,
        title: "BAE",
        dirty: false,
    };
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 1000.0))),
        events,
        ..Default::default()
    };
    let mut out = Vec::new();
    let mut full = ctx.run_ui(input, |ui| out = wb.ui(ui, &cx, |_| {}));
    full.textures_delta.clear();
    out.retain(|inv| !wb.handle(inv, &cx));
    out
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn row(action: &str, when: Option<&str>) -> RowKey {
    RowKey {
        action: action.into(),
        params: String::new(),
        when: when.map(str::to_owned),
    }
}

fn session(wb: &mut Workbench) -> &mut Session {
    wb.editor.session.as_mut().expect("the dialog is open")
}

fn chord(s: &str) -> Chord {
    Chord::parse(s).unwrap()
}

/// The tree groups commands like AE: Application (tools, then menus, then the
/// rest) and Panels (one group per panel scope, AE's names).
#[test]
fn the_tree_is_grouped_like_ae() {
    crate::i18n::set_lang(crate::i18n::Lang::En);
    let mut wb = workbench();
    wb.open_shortcut_editor();
    let s = session(&mut wb);
    let t = &s.tree;
    let at = |title: &str| {
        t.nodes
            .iter()
            .position(|n| n.title == title)
            .unwrap_or_else(|| panic!("no node {title}"))
    };
    let depth = |title: &str| t.nodes[at(title)].depth;
    // Roots.
    assert_eq!(t.nodes[0].title, "Application");
    assert_eq!(depth("Panels"), 0);
    // The tool first, then menus in menu order, submenus nested, then the rest.
    assert_eq!(t.nodes[1].title, "Selection");
    assert!(at("Edit") < at("Composition") && at("Composition") < at("Animation"));
    assert_eq!(depth("Composition Settings..."), 2);
    assert_eq!(depth("Reveal"), 2);
    assert_eq!(depth("Reveal Keyframes"), 3);
    // Every command once: in its menu, not again among the rest.
    assert_eq!(
        t.nodes
            .iter()
            .filter(|n| n.entry.as_ref().is_some_and(|e| e.id == "comp.settings"))
            .count(),
        1
    );
    // Panels: AE's scope name, with the panel's commands.
    let timeline = at("Timeline Panel");
    assert_eq!(t.nodes[timeline].depth, 1);
    assert!(t.nodes[timeline + 1].title == "Go to Previous Keyframe");
    // The Commands menu lists AE's eleven panel scopes, alphabetically.
    let scopes: Vec<&str> = t.scopes.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(scopes.len(), 11);
    assert_eq!(scopes[0], "Character and Paragraph Panels");
    assert_eq!(scopes[10], "Viewer Panel");
    // Both roots open, the groups closed; search finds a command and opens its groups.
    let visible: Vec<&str> = t
        .visible(&s.expanded, "", &s.chords)
        .iter()
        .map(|&i| t.nodes[i].title.as_str())
        .collect();
    assert!(visible.contains(&"Composition") && !visible.contains(&"New Composition..."));
    let found: Vec<&str> = t
        .visible(&s.expanded, "settings", &s.chords)
        .iter()
        .map(|&i| t.nodes[i].title.as_str())
        .collect();
    assert_eq!(
        found,
        ["Application", "Composition", "Composition Settings..."]
    );
    // … or a shortcut.
    let by_key = t.visible(&s.expanded, &text::chord_text(&chord("Cmd+N")), &s.chords);
    assert!(
        by_key
            .iter()
            .any(|&i| t.nodes[i].title == "New Composition...")
    );
}

/// Changes are a draft: OK applies and asks the shell to save, Cancel drops them.
#[test]
fn changes_apply_on_ok_only() {
    let mut wb = workbench();
    wb.handle_editor_open();
    // Record Cmd+K for New Composition: Composition Settings loses it.
    let s = session(&mut wb);
    s.select(row("comp.new", None));
    s.focus = Some(PillFocus::New);
    run(&mut wb, vec![key(Key::K, Modifiers::COMMAND)]);
    let s = session(&mut wb);
    assert_eq!(
        s.chords_of(&row("comp.new", None)),
        [chord("Cmd+N"), chord("Cmd+K")]
    );
    assert!(s.chords_of(&row("comp.settings", None)).is_empty());
    assert_eq!(s.preset, CUSTOM_PRESET, "a changed default becomes Custom");
    assert!(matches!(
        &s.message,
        Message::Conflict { kind: ConflictKind::Application, command, .. }
            if command == "Composition Settings..."
    ));
    // Not live yet.
    let live = wb.keymap().lookup(&chord("Cmd+K"), None).unwrap();
    assert_eq!(live.action, "comp.settings");
    // Undo takes the whole change back (one level).
    let mut s = wb.editor.session.take().unwrap();
    s.undo_change();
    assert_eq!(s.chords_of(&row("comp.settings", None)), [chord("Cmd+K")]);
    assert!(s.undo.is_none());
    assert_eq!(s.message, Message::Empty);
    wb.editor.session = Some(s);
    // Cancel (Esc: the first one gives up the focused pill) drops everything.
    run(&mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(wb.shortcut_editor_open());
    run(&mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!wb.shortcut_editor_open());
    assert!(!wb.take_save_request());

    // Again, with OK.
    wb.handle_editor_open();
    let s = session(&mut wb);
    s.select(row("comp.new", None));
    s.focus = Some(PillFocus::Pill(0));
    run(
        &mut wb,
        vec![key(Key::M, Modifiers::COMMAND | Modifiers::SHIFT)],
    );
    // Enter is OK when no pill waits for a key.
    session(&mut wb).focus = None;
    run(&mut wb, vec![key(Key::Enter, Modifiers::NONE)]);
    assert!(!wb.shortcut_editor_open());
    assert!(wb.take_save_request());
    let km = wb.keymap();
    assert_eq!(
        km.lookup(&chord("Cmd+Shift+M"), None).unwrap().action,
        "comp.new"
    );
    assert!(km.lookup(&chord("Cmd+N"), None).is_none(), "replaced");
    assert!(!wb.state().keymaps["km"].is_empty());

    // Reopened: the Custom preset; choosing the default one and OK resets.
    wb.handle_editor_open();
    assert_eq!(session(&mut wb).preset, CUSTOM_PRESET);
    let mut s = wb.editor.session.take().unwrap();
    s.choose_preset(DEFAULT_PRESET);
    wb.editor.session = Some(s);
    wb.commit_shortcut_editor();
    assert!(!wb.state().keymaps.contains_key("km"));
    assert_eq!(
        wb.keymap().lookup(&chord("Cmd+N"), None).unwrap().action,
        "comp.new"
    );
}

/// A focused pill takes the key; Esc gives the pill up, then closes the dialog.
#[test]
fn pills_record_and_escape_steps_back() {
    let mut wb = workbench();
    wb.handle_editor_open();
    let s = session(&mut wb);
    s.select(row("timeline.prev_key", Some("timeline")));
    s.focus = Some(PillFocus::New);
    // B is the timeline's Set Work Area Start: it moves (panel conflict).
    run(&mut wb, vec![key(Key::B, Modifiers::NONE)]);
    let s = session(&mut wb);
    assert_eq!(
        s.chords_of(&row("timeline.prev_key", Some("timeline"))),
        [chord("J"), chord("B")]
    );
    assert!(matches!(
        s.message,
        Message::Conflict {
            kind: ConflictKind::Panel,
            ..
        }
    ));
    // U is an application shortcut: the panel one overrides it, both stay.
    s.focus = Some(PillFocus::New);
    run(&mut wb, vec![key(Key::U, Modifiers::NONE)]);
    let s = session(&mut wb);
    assert!(matches!(
        s.message,
        Message::Conflict {
            kind: ConflictKind::Override,
            ..
        }
    ));
    assert_eq!(
        s.chords_of(&row("layer.reveal_keyframes", None)),
        [chord("U")]
    );
    // Clearing the focused pill removes that shortcut.
    s.focus = Some(PillFocus::Pill(0));
    let mut s = wb.editor.session.take().unwrap();
    let r = row("timeline.prev_key", Some("timeline"));
    s.remove_chord(&r, chord("J"));
    assert_eq!(s.chords_of(&r), [chord("B"), chord("U")]);
    wb.editor.session = Some(s);
    // Esc: first the pill, then the dialog.
    session(&mut wb).focus = Some(PillFocus::New);
    run(&mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(wb.shortcut_editor_open());
    assert_eq!(session(&mut wb).focus, None);
    run(&mut wb, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!wb.shortcut_editor_open());
    assert_eq!(
        wb.keymap()
            .lookup(&chord("J"), Some("timeline"))
            .unwrap()
            .action,
        "timeline.prev_key",
        "cancelled"
    );
}

/// The dialog is modal: shortcuts do not reach the project while it is open, and
/// it opens with its shortcut action.
#[test]
fn the_dialog_is_modal() {
    let mut wb = workbench();
    let stage = Stage::new();
    let author = AuthorId::system();
    let resources = Resources::new();
    let cx = WorkbenchContext {
        stage: &stage,
        author: &author,
        resources: &resources,
        title: "",
        dirty: false,
    };
    assert!(wb.handle(&Invocation::new(ids::KEYBOARD_SHORTCUTS), &cx));
    assert!(wb.shortcut_editor_open() && wb.modal_open(&resources));
    let ctx = crate::testing::ctx_with_fonts();
    run_in(&ctx, &mut wb, vec![]);
    let out = run_in(&ctx, &mut wb, vec![key(Key::V, Modifiers::NONE)]);
    assert!(out.is_empty(), "V reached the project: {out:?}");
    assert!(wb.shortcut_editor_open());
}

impl Workbench {
    fn handle_editor_open(&mut self) {
        self.open_shortcut_editor();
    }
}
