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
    ("motion.composition.new", Some("Cmd+N"), "新建合成"),
    // AE 26 shortens this item to “将合成裁剪到工作区”. BAE keeps the glossary
    // terms (issue #109): Work Area = 工作区域 (工作区 is Workspace), Trim = 修剪
    // (as in 将入点修剪到当前时间; 裁剪 is Crop).
    (
        "motion.composition.trim",
        Some("Cmd+Shift+X"),
        "将合成修剪至工作区域",
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
    ("shell.window.effects", Some("Cmd+5"), "效果和预设"),
    ("shell.window.preview", Some("Cmd+3"), "预览"),
    ("shell.window.properties", None, "属性"),
    ("shell.window.info", Some("Cmd+2"), "信息"),
    ("motion.window.render_queue", Some("Cmd+Alt+0"), "渲染队列"),
];

/// BAE-only items (no AE counterpart): Layer ▸ New ▸ Rectangle / Ellipse.
const BAE_ONLY: &[&str] = &[
    "motion.layer.new.rectangle",
    "motion.layer.new.ellipse",
    "shell.window.history",
    "shell.window.agent_activity",
    "shell.window.agent_proposals",
];

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
        .filter(|id| !known.contains(&id.as_str()))
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
    // Window items open panels: always enabled (checked below), not part of
    // what the selection changes.
    let window = |i: &MenuItemView| i.path.first().is_some_and(|p| p == "Window");
    assert!(
        h.state()
            .menu_items()
            .iter()
            .filter(|i| window(i))
            .all(|i| i.enabled)
    );
    let enabled = |h: &Ui| -> Vec<String> {
        h.state()
            .menu_items()
            .into_iter()
            .filter(|i| i.enabled && !window(i))
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
        "motion.composition.new",
    ]
    .map(String::from)
    .to_vec();
    want_none.sort();
    let mut got = none.clone();
    got.sort();
    assert_eq!(got, want_none, "enabled without a comp");

    // A comp, nothing selected: Layer ▸ New, Trim, Add to Render Queue.
    key(&mut h, Modifiers::COMMAND, Key::N);
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
    key(&mut h, Modifiers::COMMAND, Key::N);
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
        key(&mut h, Modifiers::COMMAND, Key::Y);
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
    key(&mut h, Modifiers::COMMAND, Key::N);
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
    key(&mut h, Modifiers::COMMAND, Key::N);
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
    // Every Window item runs without failing.
    for item in h
        .state()
        .menu_items()
        .into_iter()
        .filter(|i| i.path == ["Window"])
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
    assert!(h.state().workspace.modal_open());
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
    assert!(!h.state().workspace.modal_open());
    key(&mut h, Modifiers::COMMAND, Key::N);
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

/// Painted characters egui's built-in fonts cannot draw (macOS shortcut modifiers
/// excepted: drawn with SF, see `ui_text`).
fn missing_glyphs(h: &Ui) -> Vec<String> {
    painted_texts(h)
        .into_iter()
        .filter_map(|t| {
            let missing: Vec<char> = crate::ui_text::missing_from_builtin_fonts(t.chars())
                .into_iter()
                .filter(|c| !crate::ui_text::MACOS_MODIFIERS.contains(c))
                .collect();
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

/// #135: the empty Render Queue's hint named the menu path with `▸`, which egui's
/// built-in fonts do not have (a box on Windows). Nothing painted may need a glyph
/// they lack.
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

/// #136: with `BAE_LANG=zh`, Edit > Undo, the History panel and the Keyboard
/// Shortcuts window showed English command names ("撤消 New Layer") and raw panel
/// ids ("打开面板: character").
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
        "撤消 新建图层"
    );
    assert_eq!(
        crate::shell::undo_redo_text("Redo", redo.as_deref()),
        "重做"
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
    assert!(history.iter().any(|l| l == "新建图层"), "{history:?}");
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
    for (id, zh) in [
        ("character", "字符"),
        ("paragraph", "段落"),
        ("tools", "工具"),
    ] {
        let row = rows
            .iter()
            .find(|r| r.params.get("id").and_then(|v| v.as_str()) == Some(id))
            .unwrap_or_else(|| panic!("no shortcut row for the {id} panel"));
        assert_eq!(row.shown, format!("打开面板: {zh}"));
    }
}
