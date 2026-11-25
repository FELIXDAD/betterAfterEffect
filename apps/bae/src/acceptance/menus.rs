//! Menus and key routing: enabled / greyed-out state of every registered item,
//! the shortcut each shows against AE 26 (docs/reference/ae/menus.json, Ctrl →
//! Cmd), Chinese titles against AE's Simplified Chinese menus, `BAE_LANG=zh` /
//! `en`, and shortcuts with the focus in each panel (run once, not twice; text
//! fields keep their own keys).

use egui::{Key, Modifiers, pos2};

use super::*;

/// Contributed items: registry id, the chord AE shows (BAE notation), AE's
/// Simplified Chinese label (without the `(X)` mnemonic and ellipsis).
const AE_ITEMS: &[(&str, Option<&str>, &str)] = &[
    ("shell.file.new_project", Some("Cmd+Alt+N"), "新建项目"),
    ("shell.file.open_project", Some("Cmd+O"), "打开项目"),
    ("shell.file.close_project", None, "关闭项目"),
    ("shell.file.save", Some("Cmd+S"), "保存"),
    ("shell.file.save_as", Some("Cmd+Shift+S"), "另存为"),
    ("motion.file.import_file", Some("Cmd+I"), "文件"),
    (
        "motion.file.import_multiple_files",
        Some("Cmd+Alt+I"),
        "多个文件",
    ),
    ("motion.composition.new", Some("Cmd+N"), "新建合成"),
    (
        "motion.comp.composition_settings",
        Some("Cmd+K"),
        "合成设置",
    ),
    (
        "motion.layer.layer_settings",
        Some("Cmd+Shift+Y"),
        "图层设置",
    ),
    ("motion.layer.pre_compose", Some("Cmd+Shift+C"), "预合成"),
    (
        "motion.composition.trim",
        Some("Cmd+Shift+X"),
        "将合成裁剪到工作区",
    ),
    (
        "motion.composition.render_queue",
        Some("Cmd+M"),
        "添加到渲染队列",
    ),
    ("motion.layer.new.text", Some("Cmd+Alt+Shift+T"), "文本"),
    ("motion.layer.new.solid", Some("Cmd+Y"), "纯色"),
    ("motion.layer.new.light", Some("Cmd+Alt+Shift+L"), "灯光"),
    ("motion.layer.new.camera", Some("Cmd+Alt+Shift+C"), "摄像机"),
    ("motion.layer.new.null", Some("Cmd+Alt+Shift+Y"), "空对象"),
    ("motion.layer.new.shape", None, "形状图层"),
    ("motion.layer.new.adjustment", Some("Cmd+Alt+Y"), "调整图层"),
    (
        "motion.layer.arrange.front",
        Some("Cmd+Shift+CloseBracket"),
        "置于顶层",
    ),
    (
        "motion.layer.arrange.forward",
        Some("Cmd+CloseBracket"),
        "前移一层",
    ),
    (
        "motion.layer.arrange.backward",
        Some("Cmd+OpenBracket"),
        "后移一层",
    ),
    (
        "motion.layer.arrange.back",
        Some("Cmd+Shift+OpenBracket"),
        "置于底层",
    ),
    ("motion.animation.ease", Some("F9"), "缓动"),
    ("motion.animation.ease_in", Some("Shift+F9"), "缓入"),
    ("motion.animation.ease_out", Some("Cmd+Shift+F9"), "缓出"),
    ("motion.animation.reverse", None, "时间反向关键帧"),
    ("motion.animation.hold", Some("Cmd+Alt+H"), "切换定格关键帧"),
    ("motion.edit.duplicate", Some("Cmd+D"), "重复"),
    ("motion.edit.split", Some("Cmd+Shift+D"), "拆分图层"),
    // Window (issues #117, #118). AE's Effect Controls item has a dynamic label
    // (“效果控件: 图层名”); AE binds F3 to it, as BAE does.
    ("shell.window.project", Some("Cmd+0"), "项目"),
    ("shell.window.composition", None, "合成"),
    ("shell.window.timeline", None, "时间轴"),
    ("shell.window.effect_controls", Some("F3"), "效果控件"),
    // Info is a placeholder panel so far: greyed, and AE's Cmd+2 binds once
    // it is real (#shortcut sweep). Effects & Presets is real: Cmd+5.
    ("shell.window.effects", Some("Cmd+5"), "效果和预设"),
    ("shell.window.preview", Some("Cmd+3"), "预览"),
    ("shell.window.properties", None, "属性"),
    ("shell.window.info", Some("Cmd+2"), "信息"),
    ("motion.window.render_queue", Some("Cmd+Alt+0"), "渲染队列"),
];

/// BAE-only items (no AE counterpart): the History and Agent panels, the Agent
/// workspace.
const BAE_ONLY: &[&str] = &[
    "shell.window.history",
    "shell.window.agent_activity",
    "shell.window.agent_proposals",
    "shell.window.workspace.agent",
];

/// The rest of AE's menu bar (`menu.*` ids) comes from the AE command catalog,
/// whose tests compare every entry with AE's `menus.json` (bae-ui-motion).
fn from_ae_catalog(id: &str) -> bool {
    id.starts_with("menu.")
}

#[test]
fn every_menu_item_is_compared_with_ae() {
    let h = gui();
    let items = h.state().menu_items();
    let known: Vec<&str> = AE_ITEMS
        .iter()
        .map(|(id, ..)| *id)
        .chain(BAE_ONLY.iter().copied())
        .collect();
    let unknown: Vec<&String> = items
        .iter()
        .map(|i| &i.id)
        .filter(|id| !known.contains(&id.as_str()) && !from_ae_catalog(id))
        .collect();
    assert!(
        unknown.is_empty(),
        "menu items not covered by this test: {unknown:?}"
    );
    let missing: Vec<&&str> = known
        .iter()
        .filter(|k| !items.iter().any(|i| i.id == **k))
        .collect();
    assert!(missing.is_empty(), "items gone from the menu: {missing:?}");
}

#[test]
fn shortcuts_shown_in_menus_match_ae() {
    let h = gui();
    let mut mismatches = Vec::new();
    for (id, ae, _) in AE_ITEMS {
        let item = h.state().menu_item(id);
        if item.chord.as_deref() != *ae {
            mismatches.push(format!(
                "{id} ({}): BAE {:?}, AE {:?}",
                item.title, item.chord, ae
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "menu shortcuts differ from AE:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn menu_enabled_state_follows_the_selection() {
    let mut h = gui();
    // Window items of implemented panels open them: always enabled (checked
    // below), not part of what the selection changes. Panels that are only
    // placeholders are greyed, like every AE item whose command does not run
    // yet (none of the AE items listed here any more).
    let window = |i: &MenuItemView| i.path.first().is_some_and(|p| p == "Window");
    let placeholder: [&str; 0] = [];
    let ae_item = |i: &MenuItemView| AE_ITEMS.iter().any(|(id, ..)| *id == i.id);
    for i in h
        .state()
        .menu_items()
        .iter()
        .filter(|i| window(i) && ae_item(i))
    {
        assert_eq!(i.enabled, !placeholder.contains(&i.id.as_str()), "{}", i.id);
    }
    // The items this test follows: AE_ITEMS outside Window.
    let enabled = |h: &Ui| -> Vec<String> {
        h.state()
            .menu_items()
            .into_iter()
            .filter(|i| i.enabled && !window(i) && ae_item(i))
            .map(|i| i.id)
            .collect()
    };
    // No comp: File items and New Composition only.
    let none = enabled(&h);
    let mut want_none: Vec<String> = [
        "shell.file.new_project",
        "shell.file.open_project",
        "shell.file.close_project",
        "shell.file.save",
        "shell.file.save_as",
        "motion.file.import_file",
        "motion.file.import_multiple_files",
        "motion.composition.new",
    ]
    .map(String::from)
    .to_vec();
    want_none.sort();
    let mut got = none.clone();
    got.sort();
    assert_eq!(got, want_none, "enabled without a comp");

    // A comp, nothing selected: Layer ▸ New, Trim, Add to Render Queue.
    new_comp(&mut h);
    let comp = enabled(&h);
    for id in [
        "motion.layer.new.solid",
        "motion.layer.new.camera",
        "motion.composition.trim",
        "motion.composition.render_queue",
    ] {
        assert!(
            comp.iter().any(|e| e == id),
            "{id} disabled with a comp open"
        );
    }
    for id in [
        "motion.edit.duplicate",
        "motion.layer.arrange.front",
        "motion.animation.ease",
    ] {
        assert!(
            !comp.iter().any(|e| e == id),
            "{id} enabled with nothing selected"
        );
    }

    // A selected layer: Arrange, Duplicate, Split; not the keyframe items.
    assert!(h.state_mut().menu("motion.layer.new.solid"));
    settle(&mut h);
    confirm_dialog(&mut h);
    let layer = enabled(&h);
    for id in [
        "motion.edit.duplicate",
        "motion.edit.split",
        "motion.layer.arrange.back",
    ] {
        assert!(
            layer.iter().any(|e| e == id),
            "{id} disabled with a layer selected"
        );
    }
    for id in [
        "motion.animation.ease",
        "motion.animation.hold",
        "motion.animation.reverse",
    ] {
        assert!(
            !layer.iter().any(|e| e == id),
            "{id} enabled without selected keyframes"
        );
    }

    // Selected keyframes: the Animation items.
    let l = h.state().layers()[0].clone();
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    let t = bae_model::Time::ZERO;
    h.state().session().lock().selected_keys =
        vec![bae_motion::KeyRef::new(l, "transform:position", t)];
    let keys = enabled(&h);
    for id in [
        "motion.animation.ease",
        "motion.animation.ease_in",
        "motion.animation.hold",
    ] {
        assert!(
            keys.iter().any(|e| e == id),
            "{id} disabled with a keyframe selected"
        );
    }
    // F9 through the menu applies Easy Ease (a model command runs, no failure).
    assert!(h.state_mut().menu("motion.animation.ease"));
    settle(&mut h);
    assert!(h.state().model_commands().iter().any(|c| c == "key.ease"));
    assert!(
        h.state()
            .outcomes
            .iter()
            .all(|(_, o)| !o.starts_with("failed")),
        "{:?}",
        h.state().outcomes
    );
}

/// Strips AE's mnemonic `(X)` and trailing ellipsis.
fn ae_label(label: &str) -> String {
    let mut s = label.to_owned();
    if let Some(i) = s.find('(') {
        s.truncate(i);
    }
    s.trim_end_matches(['.', '…']).trim().to_owned()
}

#[test]
fn chinese_menus_use_ae_terms() {
    if !in_child(
        "acceptance::menus::chinese_menus_use_ae_terms",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    assert_eq!(bae_ui::i18n::lang(), bae_ui::Lang::Zh);
    let h = gui();
    let mut wrong = Vec::new();
    for (id, _, ae) in AE_ITEMS {
        let item = h.state().menu_item(id);
        let shown = ae_label(bae_ui::tr(&item.title));
        if shown != *ae {
            wrong.push(format!("{id}: BAE “{shown}”, AE “{ae}”"));
        }
        for part in &item.path {
            let zh = bae_ui::tr(part);
            assert!(!zh.is_ascii(), "menu {part:?} not translated");
        }
    }
    // Shell menus and dialogs.
    for en in [
        "File",
        "Edit",
        "Window",
        "Help",
        "Undo",
        "Redo",
        "Don't Save",
        "Could not open the project.",
    ] {
        assert!(!bae_ui::tr(en).is_ascii(), "{en:?} not translated");
    }
    // Panels paint Chinese.
    let texts = painted_texts(&h);
    for zh in ["项目", "时间轴"] {
        assert!(
            texts.iter().any(|t| t.contains(zh)),
            "{zh} not shown: {texts:?}"
        );
    }
    assert!(
        wrong.is_empty(),
        "Chinese menu titles differ from AE:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn english_ui_has_no_chinese() {
    if !in_child(
        "acceptance::menus::english_ui_has_no_chinese",
        &[("BAE_LANG", "en")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    let cjk: Vec<String> = painted_texts(&h)
        .into_iter()
        .filter(|t| t.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)))
        .collect();
    assert!(cjk.is_empty(), "Chinese in the English UI: {cjk:?}");
    for item in h.state().menu_items() {
        assert!(bae_ui::tr(&item.title).is_ascii(), "{}", item.title);
    }
}

/// Clicks an empty part of the panel whose tab or header shows `anchor`.
fn focus_panel(h: &mut Ui, anchor: &str, offset: egui::Vec2) {
    let at =
        find_text(h, anchor).unwrap_or_else(|| panic!("no {anchor:?}: {:?}", painted_texts(h)));
    click_at(h, at.center() + offset);
}

#[test]
fn shortcuts_run_once_whichever_panel_has_focus() {
    let mut h = gui();
    new_comp(&mut h);
    let targets: [(&str, egui::Vec2); 5] = [
        ("Name", egui::vec2(0.0, 420.0)), // Project panel, below the rows
        ("Layer Name", egui::vec2(700.0, 120.0)), // Timeline graph
        ("Fit", egui::vec2(300.0, -250.0)), // Composition viewer
        ("Effects & Presets", egui::vec2(0.0, 40.0)), // Effects & Presets panel
        ("Render", egui::vec2(200.0, 0.0)), // Render Queue
    ];
    for (anchor, offset) in targets {
        if anchor == "Render" {
            // Cmd+M opens the Render Queue (a tab over the Project panel).
            key(&mut h, Modifiers::COMMAND, Key::M);
        }
        assert!(
            find_text(&h, anchor).is_some(),
            "panel anchor {anchor:?} not visible"
        );
        focus_panel(&mut h, anchor, offset);
        let layers = h.state().layers().len();
        let dispatched = h.state().dispatched.len();
        new_solid(&mut h);
        let new_layer_runs = h.state().dispatched[dispatched..]
            .iter()
            .filter(|i| i.id == "motion.new_layer")
            .count();
        assert_eq!(
            new_layer_runs, 1,
            "focus {anchor}: Cmd+Y dispatched {new_layer_runs} times"
        );
        assert_eq!(
            h.state().layers().len(),
            layers + 1,
            "focus {anchor}: Cmd+Y"
        );
        // Undo with the same focus.
        key(&mut h, Modifiers::COMMAND, Key::Z);
        assert_eq!(h.state().layers().len(), layers, "focus {anchor}: Cmd+Z");
        key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        assert_eq!(
            h.state().layers().len(),
            layers + 1,
            "focus {anchor}: Cmd+Shift+Z"
        );
    }
}

#[test]
fn text_fields_keep_delete_undo_and_select_all() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    // The Project panel's search field (the only text input there).
    let search = h
        .root()
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .expect("project search field")
        .rect();
    click_at(&mut h, search.center());
    assert!(
        h.ctx.egui_wants_keyboard_input(),
        "search field did not take focus"
    );
    type_text(&mut h, "abc");
    // Backspace / Delete edit the text; the selected layer stays.
    key(&mut h, Modifiers::NONE, Key::Backspace);
    key(&mut h, Modifiers::NONE, Key::Delete);
    assert!(
        h.state().stage().prim(&layer).is_some(),
        "Backspace in a text field deleted the layer"
    );
    // Cmd+Z undoes typing, not the project.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(
        h.state().stage().prim(&layer).is_some(),
        "Cmd+Z in a text field undid the project"
    );
    // Cmd+A selects the text, not all layers; J/K/Space/P are typed.
    key(&mut h, Modifiers::COMMAND, Key::A);
    let dispatched = h.state().dispatched.len();
    for k in [Key::J, Key::K, Key::P, Key::Space, Key::U] {
        press(&mut h, Modifiers::NONE, k);
    }
    settle(&mut h);
    let ran: Vec<String> = h.state().dispatched[dispatched..]
        .iter()
        .map(|i| i.id.clone())
        .collect();
    assert!(ran.is_empty(), "typing keys ran shortcuts: {ran:?}");
    assert!(!h.state().is_playing());
    // Escape leaves the field; with the timeline focused, Delete deletes the
    // selected layer (AE).
    key(&mut h, Modifiers::NONE, Key::Escape);
    let head = find_text(&h, "Layer Name").unwrap();
    click_at(&mut h, pos2(head.left() + 800.0, head.bottom() + 80.0));
    h.state().session().lock().selected_layers = vec![layer.clone()];
    key(&mut h, Modifiers::NONE, Key::Delete);
    assert!(
        h.state().stage().prim(&layer).is_none(),
        "Delete outside text fields did not delete"
    );
}

use egui_kittest::kittest::Queryable as _;

// ---- Retest of #109 / #112 / #117 / #118 (dev 763eaad) ----

fn render_queue_open(h: &Ui) -> bool {
    h.query_by_label("Render").is_some()
}

#[test]
fn window_render_queue_item_opens_and_closes_the_panel() {
    let mut h = gui();
    let item = h.state().menu_item("motion.window.render_queue");
    assert!(item.enabled, "Window ▸ Render Queue disabled");
    assert_eq!(item.path, ["Window"]);
    assert_eq!(item.chord.as_deref(), Some("Cmd+Alt+0"));
    assert!(!render_queue_open(&h), "Render Queue open at start");
    // Menu click opens it (no comp needed), a second click closes it (AE's
    // checked Window items), Cmd+Alt+0 opens it again.
    assert!(h.state_mut().menu("motion.window.render_queue"));
    settle(&mut h);
    assert!(
        render_queue_open(&h),
        "Window ▸ Render Queue did not open the panel"
    );
    assert!(h.state_mut().menu("motion.window.render_queue"));
    settle(&mut h);
    assert!(!render_queue_open(&h), "second click did not close it");
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::Num0);
    assert!(render_queue_open(&h), "Cmd+Alt+0 did not open it");
    // Closing it with items queued keeps the items (runtime queue).
    new_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::M);
    assert!(h.state_mut().menu("motion.window.render_queue"));
    settle(&mut h);
    assert!(h.state_mut().menu("motion.window.render_queue"));
    settle(&mut h);
    assert!(render_queue_open(&h));
    assert_eq!(h.state().render_queue().lock().items().len(), 1);
}

#[test]
fn window_panel_items_toggle_and_reopen_panels() {
    let mut h = gui();
    new_comp(&mut h);
    // Project: Cmd+0 / Window ▸ Project toggle the Project panel.
    let shown = |h: &Ui| find_text(h, "Name").is_some() && find_text(h, "Type").is_some();
    assert!(shown(&h));
    assert!(h.state_mut().menu("shell.window.project"));
    settle(&mut h);
    assert!(
        !shown(&h),
        "Window ▸ Project did not close the Project panel"
    );
    key(&mut h, Modifiers::COMMAND, Key::Num0);
    assert!(shown(&h), "Cmd+0 did not reopen the Project panel");
    // Every enabled Window item runs without failing (the others are AE's
    // panels BAE does not have yet, greyed).
    for item in h
        .state()
        .menu_items()
        .into_iter()
        .filter(|i| i.path == ["Window"] && i.enabled)
    {
        assert!(h.state_mut().menu(&item.id), "{} disabled", item.id);
        settle(&mut h);
        assert!(h.state_mut().menu(&item.id));
        settle(&mut h);
    }
    let failed: Vec<_> = h
        .state()
        .outcomes
        .iter()
        .filter(|(_, o)| o.starts_with("failed") || o == "unknown")
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

#[test]
fn undo_redo_and_shell_items_show_their_keymap_chords() {
    // AppMenu::sync shows what the keymap binds to each shell item (#117).
    let h = gui();
    let chord = |id: &str| {
        h.state()
            .workspace
            .keymap()
            .chord_for_invocation(&Invocation::new(id))
            .filter(|c| !c.is_typing_key())
            .map(|c| c.to_string())
    };
    assert_eq!(chord(ids::UNDO).as_deref(), Some("Cmd+Z"));
    assert_eq!(chord(ids::REDO).as_deref(), Some("Cmd+Shift+Z"));
    assert!(
        chord(ids::KEYBOARD_SHORTCUTS).is_some(),
        "Keyboard Shortcuts has no chord"
    );
    println!(
        "Keyboard Shortcuts: {:?}, Quit: {:?}",
        chord(ids::KEYBOARD_SHORTCUTS),
        chord(ids::QUIT)
    );
    // While a text field is focused the shell hands Undo / Redo back to it
    // (`Shell::user_event` checks `text_edit_focused`).
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let search = h
        .root()
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .rect();
    click_at(&mut h, search.center());
    assert!(h.ctx.text_edit_focused());
}

#[test]
fn welcome_dialog_blocks_panel_keys_and_reports_modal() {
    let mut g = Gui::new();
    g.workspace.show_role_picker();
    let mut h = gui_with(g);
    settle(&mut h);
    // `Workspace::modal_open` is what the shell checks for native menu commands.
    assert!(h.state().workspace.modal_open(h.state().app.resources()));
    for (m, k) in [
        (Modifiers::COMMAND, Key::N),
        (Modifiers::COMMAND, Key::Y),
        (Modifiers::NONE, Key::Space),
        (Modifiers::NONE, Key::Enter),
        (Modifiers::NONE, Key::Delete),
        (Modifiers::COMMAND, Key::S),
        (Modifiers::COMMAND, Key::M),
    ] {
        key(&mut h, m, k);
    }
    assert!(
        h.state().active_comp().is_none(),
        "a shortcut reached the project"
    );
    assert!(!h.state().doc.is_dirty());
    assert!(
        h.state().dialogs.suggested.is_empty(),
        "Cmd+S opened Save As behind the modal"
    );
    assert!(find_text(&h, "Welcome to BAE").is_some());
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!h.state().workspace.modal_open(h.state().app.resources()));
    new_comp(&mut h);
    assert!(
        h.state().active_comp().is_some(),
        "shortcuts dead after the dialog closed"
    );
}

#[test]
fn chinese_window_items_and_render_messages() {
    if !in_child(
        "acceptance::menus::chinese_window_items_and_render_messages",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let h = gui();
    let zh = |id: &str| bae_ui::tr(&h.state().menu_item(id).title).to_owned();
    assert_eq!(zh("motion.window.render_queue"), "渲染队列");
    assert_eq!(zh("motion.layer.arrange.front"), "置于顶层");
    for en in [
        "Choose where to render: Output To is empty.",
        "The frames are outside the composition (0–{last}).",
        "Undo",
        "Redo",
        "Keyboard Shortcuts",
    ] {
        assert!(!bae_ui::tr(en).is_ascii(), "{en:?} not translated");
    }
    println!(
        "empty output (zh): {}",
        bae_ui::tr("Choose where to render: Output To is empty.")
    );
}

/// Painted characters the UI's system fonts cannot draw.
fn missing_glyphs(h: &Ui) -> Vec<String> {
    painted_texts(h)
        .into_iter()
        .filter_map(|t| {
            let missing = crate::ui_text::missing_from_ui_fonts(t.chars());
            (!missing.is_empty()).then(|| format!("{missing:?} in {t:?}"))
        })
        .collect()
}

fn open_empty_render_queue() -> Ui {
    let mut h = gui();
    assert!(h.state_mut().menu("motion.window.render_queue"));
    settle(&mut h);
    // Opened: the callers find its hint (in either language).
    h
}

/// #135: the empty Render Queue's hint named the menu path with `▸`, which the UI's
/// fonts did not have on Windows (a box). Nothing painted may need a glyph they lack.
#[test]
fn render_queue_hint_has_no_missing_glyphs() {
    let h = open_empty_render_queue();
    let texts = painted_texts(&h);
    assert!(
        texts
            .iter()
            .any(|t| t == "Add a composition with Composition > Add to Render Queue."),
        "hint not shown: {texts:?}"
    );
    let missing = missing_glyphs(&h);
    assert!(missing.is_empty(), "missing glyphs: {missing:#?}");
}

#[test]
fn render_queue_hint_has_no_missing_glyphs_in_chinese() {
    if !in_child(
        "acceptance::menus::render_queue_hint_has_no_missing_glyphs_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let h = open_empty_render_queue();
    let texts = painted_texts(&h);
    assert!(
        texts
            .iter()
            .any(|t| t == "用“合成 > 添加到渲染队列”添加合成。"),
        "hint not shown: {texts:?}"
    );
    let missing = missing_glyphs(&h);
    assert!(missing.is_empty(), "missing glyphs: {missing:#?}");
}

/// Latin words left in Chinese text, product names aside.
fn english_words(zh: &str) -> Option<String> {
    let mut rest = zh.to_owned();
    for name in crate::ui_text::SAME_IN_CHINESE {
        rest = rest.replace(name, "");
    }
    rest.chars()
        .any(|c| c.is_ascii_alphabetic())
        .then(|| zh.to_owned())
}

/// #136 / #140: with `BAE_LANG=zh`, Edit > Undo, the History panel and the Keyboard
/// Shortcuts window showed English command names ("撤消 New Layer") and raw panel
/// ids ("打开面板: character"); variants of one command (PageUp / PageDown /
/// Shift+PageUp, Cmd+[ / Cmd+], Alt+Shift+P / S …) shared one name.
#[test]
fn chinese_ui_names_every_command_and_panel() {
    if !in_child(
        "acceptance::menus::chinese_ui_names_every_command_and_panel",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);

    let (undo, redo) = h.state().doc.undo_state();
    assert_eq!(
        crate::shell::undo_redo_text("Undo", undo.as_deref()),
        "撤消 新建纯色"
    );
    // Nothing to redo: AE says “无法重做”.
    assert_eq!(
        crate::shell::undo_redo_text("Redo", redo.as_deref()),
        "无法重做"
    );

    let log = h
        .state()
        .app
        .resource::<Arc<bae_ui::history::HistoryLog>>()
        .cloned()
        .expect("history log");
    let history: Vec<String> = log
        .entries()
        .iter()
        .map(|e| bae_ui::tr(&e.label).to_owned())
        .collect();
    assert!(history.iter().any(|l| l == "新建纯色"), "{history:?}");
    let english: Vec<String> = history.iter().filter_map(|l| english_words(l)).collect();
    assert!(english.is_empty(), "History shows English: {english:?}");

    let rows = h.state().workspace.shortcut_rows();
    let english: Vec<String> = rows
        .iter()
        .filter_map(|r| english_words(&r.shown).map(|s| format!("{} {}: {s}", r.id, r.params)))
        .collect();
    assert!(
        english.is_empty(),
        "Keyboard Shortcuts shows English: {english:#?}"
    );
    let shown = |id: &str, params: serde_json::Value| {
        rows.iter()
            .find(|r| r.id == id && r.params == params)
            .map(|r| r.shown.clone())
            .unwrap_or_else(|| panic!("no shortcut row {id} {params}"))
    };
    for (frames, zh) in [
        (-1, "上一帧"),
        (1, "下一帧"),
        (-10, "后退 10 帧"),
        (10, "前进 10 帧"),
    ] {
        assert_eq!(
            shown("motion.step", serde_json::json!({ "frames": frames })),
            zh
        );
    }
    for (to, zh) in [
        ("forward", "前移一层"),
        ("backward", "后移一层"),
        ("front", "置于顶层"),
        ("back", "置于底层"),
    ] {
        assert_eq!(shown("motion.order", serde_json::json!({ "to": to })), zh);
    }
    assert_eq!(
        shown(
            "motion.toggle_key",
            serde_json::json!({ "attr": "transform:position" })
        ),
        "位置：添加或移除关键帧"
    );
    assert_eq!(
        shown("motion.new_layer", serde_json::json!({ "kind": "solid" })),
        "新建纯色"
    );
    let mut names: Vec<&str> = rows.iter().map(|r| r.shown.as_str()).collect();
    names.sort_unstable();
    let total = names.len();
    names.dedup();
    assert_eq!(names.len(), total, "shortcut rows share a name");
    // Panels with a shortcut (placeholder panels get AE's Cmd+1 … only once
    // they are real).
    for (id, zh) in [
        ("project", "项目"),
        ("preview", "预览"),
        ("render_queue", "渲染队列"),
    ] {
        let row = rows
            .iter()
            .find(|r| r.params.get("id").and_then(|v| v.as_str()) == Some(id))
            .unwrap_or_else(|| panic!("no shortcut row for the {id} panel"));
        assert_eq!(row.shown, format!("打开面板: {zh}"));
    }
}

// ---- Retest of #135 / #136 / #140 (dev d95480a) ----
use serde_json::json;

/// Many screens in one session: comp with layers and revealed properties, the
/// Render Queue in every state, the Keyboard Shortcuts and About windows, the
/// welcome dialog, the lock notice, the review and agent workspaces (History).
fn walk_screens(h: &mut Ui, mut each: impl FnMut(&str, &Ui)) {
    each("empty project", h);
    comp_with(
        h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.text",
            "motion.layer.new.null",
        ],
    );
    key(h, Modifiers::COMMAND, Key::A);
    key(h, Modifiers::NONE, Key::P);
    key(h, Modifiers::SHIFT, Key::S);
    each("comp with revealed properties", h);
    key(h, Modifiers::COMMAND, Key::M);
    let id = h.state().render_queue().lock().items()[0].id;
    let tmp = std::env::temp_dir().join("glyphs_[####].png");
    for status in [
        bae_ui_motion::render_queue::ItemStatus::Rendering { done: 3, total: 30 },
        bae_ui_motion::render_queue::ItemStatus::Done {
            files: 30,
            first: tmp.clone(),
            seconds: 1.2,
        },
        bae_ui_motion::render_queue::ItemStatus::Stopped {
            done: 3,
            total: 30,
            first: Some(tmp.clone()),
        },
        bae_ui_motion::render_queue::ItemStatus::Failed("No space left on device".into()),
    ] {
        h.state().render_queue().lock().item_mut(id).unwrap().status = status;
        settle(h);
        each("render queue item", h);
    }
    // A refused edit: the status bar notice.
    let layer = h.state().layers()[0].clone();
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        json!({"path": layer, "attr": "locked", "value": {"type": "bool", "value": true}}),
    ));
    settle(h);
    h.state_mut().invoke(Invocation::with_params(
        "layer.delete",
        json!({"layers": [layer]}),
    ));
    settle(h);
    each("lock notice", h);
    h.state_mut()
        .invoke(Invocation::new(ids::KEYBOARD_SHORTCUTS));
    settle(h);
    each("keyboard shortcuts", h);
    key(h, Modifiers::NONE, Key::Escape);
    h.state_mut().invoke(Invocation::new(ids::ABOUT));
    settle(h);
    each("about", h);
    for ws in ["review", "agent"] {
        h.state_mut().invoke(Invocation::with_params(
            ids::SWITCH_WORKSPACE,
            json!({ "id": ws }),
        ));
        settle(h);
        each(ws, h);
    }
    h.state_mut().workspace.show_role_picker();
    settle(h);
    each("welcome dialog", h);
}

fn glyph_problems() -> Vec<String> {
    let mut h = gui();
    let mut out = Vec::new();
    walk_screens(&mut h, |screen, h| {
        for m in missing_glyphs(h) {
            out.push(format!("{screen}: {m}"));
        }
    });
    out.sort();
    out.dedup();
    out
}

#[test]
fn no_missing_glyphs_on_any_screen() {
    let problems = glyph_problems();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn no_missing_glyphs_on_any_screen_in_chinese() {
    if !in_child(
        "acceptance::menus::no_missing_glyphs_on_any_screen_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let problems = glyph_problems();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Names users type (layers, comps) in scripts the UI's system fonts may not cover:
/// what is drawn as a box.
#[test]
fn user_names_in_other_scripts_and_emoji() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let mut report = Vec::new();
    for name in [
        "片头 中文",
        "日本語 かな カナ",
        "한국어",
        "Ελληνικά Кириллица",
        "ñ é ü ß ø",
        "✨ 🎬 ❤️",
        "العربية",
    ] {
        h.state_mut().invoke(Invocation::with_params(
            "attr.set",
            json!({"path": layer, "attr": "name", "value": {"type": "string", "value": name}}),
        ));
        settle(&mut h);
        let missing = crate::ui_text::missing_from_ui_fonts(name.chars());
        report.push(format!("{name}: {missing:?}"));
    }
    println!("{}", report.join("\n"));
}

/// The undo label after each kind of edit, translated as the Edit menu shows it
/// (`tr("Undo") + tr(label)`), and the History panel's entries.
fn undo_labels(h: &mut Ui) -> Vec<String> {
    let mut labels = Vec::new();
    let mut step = |h: &mut Ui, what: &str| {
        let label = h.state().doc.undo_state().0.unwrap_or_default();
        labels.push(format!("{what} → {label}"));
    };
    new_comp(h);
    step(h, "new comp");
    new_solid(h);
    step(h, "new solid");
    key(
        h,
        Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT,
        Key::Y,
    );
    step(h, "new null");
    key(
        h,
        Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT,
        Key::T,
    );
    step(h, "new text");
    assert!(add_layer(h, "shape:rect"));
    settle(h);
    step(h, "new rectangle");
    key(h, Modifiers::ALT | Modifiers::SHIFT, Key::P);
    step(h, "keyframe");
    key(h, Modifiers::COMMAND, Key::CloseBracket);
    step(h, "bring forward");
    key(h, Modifiers::COMMAND, Key::D);
    step(h, "duplicate");
    for _ in 0..10 {
        press(h, Modifiers::NONE, Key::PageDown);
    }
    key(h, Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    step(h, "split");
    key(h, Modifiers::NONE, Key::B);
    step(h, "work area");
    let layers = h.state().layers();
    h.state_mut().invoke(Invocation::with_params(
        "layer.set_parent",
        json!({"layers": [layers[0]], "parent": layers[1]}),
    ));
    settle(h);
    step(h, "set parent");
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        json!({"path": layers[1], "attr": "name", "value": {"type": "string", "value": "改名"}}),
    ));
    settle(h);
    step(h, "rename");
    labels
}

#[test]
fn undo_and_history_names_are_chinese() {
    if !in_child(
        "acceptance::menus::undo_and_history_names_are_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    let labels = undo_labels(&mut h);
    let mut untranslated = Vec::new();
    for l in &labels {
        let label = l.split(" → ").nth(1).unwrap_or_default();
        let shown = format!("{} {}", bae_ui::tr("Undo"), bae_ui::tr(label));
        if label.is_empty()
            || !shown
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
            || bae_ui::tr(label) == label
        {
            untranslated.push(format!("{l} (shown “{shown}”)"));
        }
    }
    println!("undo labels: {labels:#?}");
    // The History panel (review workspace) shows the same entries in Chinese.
    h.state_mut().invoke(Invocation::with_params(
        ids::SWITCH_WORKSPACE,
        json!({ "id": "review" }),
    ));
    settle(&mut h);
    let english: Vec<String> = painted_texts(&h)
        .into_iter()
        .filter(|t| {
            labels.iter().any(|l| {
                let label = l.split(" → ").nth(1).unwrap_or_default();
                !label.is_empty() && t.contains(label)
            })
        })
        .collect();
    assert!(
        untranslated.is_empty(),
        "undo labels without Chinese: {untranslated:#?}"
    );
    assert!(
        english.is_empty(),
        "History shows English labels: {english:?}"
    );
}

#[test]
fn undo_labels_name_the_variant_in_english() {
    let mut h = gui();
    let labels = undo_labels(&mut h);
    let get = |what: &str| {
        labels
            .iter()
            .find(|l| l.starts_with(what))
            .cloned()
            .unwrap_or_default()
    };
    assert!(get("new solid").ends_with("New Solid"), "{labels:#?}");
    assert!(get("new null").ends_with("New Null Object"), "{labels:#?}");
    for l in &labels {
        assert!(!l.ends_with("→ "), "no undo label: {l}");
        assert!(
            !l.contains('.') || l.contains("Comp"),
            "raw command id in {l}"
        );
    }
}

/// #140: the rows of a command's variants say which variant they are.
#[test]
fn shortcut_variant_rows_say_what_they_do() {
    let h = gui();
    let rows = h.state().workspace.shortcut_rows();
    let find = |id: &str, params: serde_json::Value| {
        rows.iter()
            .find(|r| {
                r.id == id
                    && params
                        .as_object()
                        .unwrap()
                        .iter()
                        .all(|(k, v)| r.params.get(k) == Some(v))
            })
            .unwrap_or_else(|| panic!("no row {id} {params}"))
            .shown
            .clone()
    };
    let checks: Vec<(String, &str)> = vec![
        (find("motion.step", json!({"frames": -1})), "Previous"),
        (find("motion.step", json!({"frames": 1})), "Next"),
        (find("motion.step", json!({"frames": 10})), "10"),
        (find("motion.step", json!({"frames": -10})), "10"),
        (find("motion.order", json!({"to": "backward"})), "Backward"),
        (find("motion.order", json!({"to": "back"})), "Back"),
        (
            find("motion.toggle_key", json!({"attr": "transform:scale"})),
            "Scale",
        ),
        (
            find("motion.toggle_key", json!({"attr": "transform:opacity"})),
            "Opacity",
        ),
        (
            find("motion.new_layer", json!({"kind": "camera"})),
            "Camera",
        ),
    ];
    let wrong: Vec<String> = checks
        .iter()
        .filter(|(shown, want)| !shown.contains(want))
        .map(|(shown, want)| format!("“{shown}” lacks {want:?}"))
        .collect();
    let raw: Vec<&String> = rows
        .iter()
        .map(|r| &r.shown)
        .filter(|s| {
            s.contains('.')
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
        })
        .collect();
    assert!(
        wrong.is_empty() && raw.is_empty(),
        "{wrong:#?}\nraw ids: {raw:?}"
    );
}
