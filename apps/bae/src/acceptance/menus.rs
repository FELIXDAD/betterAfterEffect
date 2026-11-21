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
    key(&mut h, Modifiers::COMMAND, Key::M); // opens the Render Queue panel too
    let targets: [(&str, egui::Vec2); 5] = [
        ("Name", egui::vec2(0.0, 420.0)), // Project panel, below the rows
        ("Layer Name", egui::vec2(700.0, 120.0)), // Timeline graph
        ("Fit", egui::vec2(300.0, -250.0)), // Composition viewer
        ("Effects & Presets", egui::vec2(0.0, 40.0)), // Effects & Presets panel
        ("Render", egui::vec2(200.0, 0.0)), // Render Queue
    ];
    for (anchor, offset) in targets {
        if find_text(&h, anchor).is_none() {
            println!("panel anchor {anchor:?} not visible; skipped");
            continue;
        }
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
