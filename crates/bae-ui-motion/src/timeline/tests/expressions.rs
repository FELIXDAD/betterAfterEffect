//! Expressions: add, pick whip, multi-line edit, the language menu, editor preferences, expression commands.

use super::*;

#[test]
fn expression_fields_use_the_expression_editor_preferences() {
    let mut resources = Resources::new();
    let (font, text, bg) = crate::common::expression_style(&resources);
    assert_eq!(font.size, 12.0, "AE default without preferences");
    assert_eq!(bg, egui::Color32::from_rgb(0x1d, 0x1d, 0x1d));
    let mut prefs = bae_prefs::Preferences::default();
    prefs.set(bae_prefs::keys::EXPR_FONT_SIZE, 20);
    prefs.set("scripting_expressions.expr_color_default", "#ff0000");
    resources.insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
    let (font, text2, _) = crate::common::expression_style(&resources);
    assert_eq!(font.size, 20.0);
    assert_ne!(text, text2);
    assert_eq!(text2, egui::Color32::from_rgb(0xff, 0, 0));
}

#[test]
fn expressions_add_pick_whip_switch_and_multiline_edit() {
    use crate::actions::ids;
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let layer = h.layer.clone();
    let expr = |h: &H| {
        query::attr_spec(&h.stage, &layer, POS)
            .and_then(|s| s.expression.clone())
            .map(|e| (e.source, e.enabled))
    };
    // Animation > Add Expression on the selected Position: AE's default text.
    h.session().lock().selected_props = vec![(layer.clone(), POS.to_owned())];
    let mut tl = h.bars();
    let r = h.action(&mut tl, Invocation::new(ids::ADD_EXPRESSION));
    h.apply(&r);
    assert_eq!(expr(&h), Some(("transform.position".into(), true)));
    // Pick whip onto Rotation (one value into two): AE's temp spread.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let row = |tl: &TimelinePanel| tl.last_expr.iter().find(|e| e.1 == POS).cloned().unwrap();
    let (_, _, _, whip) = row(&tl);
    let rot = bar_key(&tl, &h.key(ROT, 1.0));
    let r = h.drag(&mut tl, whip, rot, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(
        expr(&h),
        Some(("temp = transform.rotation;\n[temp, temp]".into(), true))
    );
    // Two lines: the expression takes two rows, Rotation moves down one.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let rot2 = bar_key(&tl, &h.key(ROT, 1.0));
    assert!(rot2.y > rot.y + 10.0, "{rot:?} {rot2:?}");
    // The = switch disables it.
    let (_, _, eq, _) = row(&tl);
    let r = h.click(&mut tl, eq, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(expr(&h).map(|e| e.1), Some(false));
    // Edit: Return adds a line, keypad Enter ends editing (one step).
    let field = pos2(tl.last_axis.unwrap().left + 60.0, eq.y);
    h.click(&mut tl, field, Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![
            Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: cmd(),
            },
            Event::Text("time".into()),
        ],
        Modifiers::NONE,
    );
    bae_ui::set_keypad_enter(false);
    h.frame(
        &mut tl,
        vec![press(egui::Key::Enter), Event::Text("+ 1".into())],
        Modifiers::NONE,
    );
    bae_ui::set_keypad_enter(true);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    bae_ui::set_keypad_enter(false);
    h.apply(&r);
    assert_eq!(expr(&h), Some(("time\n+ 1".into(), false)));
    // Alt+Shift+= again removes it; undo brings it back.
    let r = h.action(&mut tl, Invocation::new(ids::ADD_EXPRESSION));
    h.apply(&r);
    assert_eq!(expr(&h), None);
    h.undo();
    assert_eq!(expr(&h), Some(("time\n+ 1".into(), false)));
}

#[test]
fn pick_whip_references_follow_ae() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let a = h.layer.clone();
    let b = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let name = query::display_name_of(&h.stage, &b);
    let r = |from: &PrimPath, to: &PrimPath, attr: Option<&str>| {
        crate::expr::reference(&h.stage, from, to, attr).unwrap()
    };
    assert_eq!(r(&a, &a, Some(ROT)), "transform.rotation");
    assert_eq!(
        r(&a, &b, Some(POS)),
        format!("thisComp.layer(\"{name}\").transform.position")
    );
    assert_eq!(r(&a, &b, None), format!("thisComp.layer(\"{name}\")"));
    assert_eq!(r(&a, &a, None), "thisLayer");
    assert_eq!(
        crate::expr::default_expression(&h.stage, &a, "transform:opacity"),
        "transform.opacity"
    );
}

#[test]
fn expression_language_menu_groups_follow_ae() {
    let groups = crate::expr_menu::groups();
    let names: Vec<&str> = groups.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "Global",
            "Vector Math",
            "Random Numbers",
            "Interpolation",
            "Color Conversion",
            "Other Math",
            "JavaScript Math",
            "Comp",
            "Footage",
            "Layer",
            "Camera",
            "Light",
            "Effect",
            "Path Property",
            "Property",
            "Key",
            "MarkerKey",
            "Project",
            "Text",
        ]
    );
    for done in [
        "Global",
        "Random Numbers",
        "Interpolation",
        "Comp",
        "Layer",
        "Property",
    ] {
        assert!(groups.iter().any(|(n, c)| *n == done && *c > 0), "{done}");
    }
    // Pressing the menu button while editing keeps the field's selection: the
    // item replaces it.
    let mut h = H::new();
    let layer = h.layer.clone();
    h.run(
        "attr.set_expression",
        json!({"path": layer, "attr": POS, "source": "value"}),
    );
    let mut tl = h.bars();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let (_, _, eq, _) = tl.last_expr[0].clone();
    let field = pos2(tl.last_axis.unwrap().left + 60.0, eq.y);
    h.click(&mut tl, field, Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: cmd(),
        }],
        Modifiers::NONE,
    );
    let menu = tl.last_lang[0].2;
    h.click(&mut tl, menu, Modifiers::NONE);
    assert_eq!(tl.lang_insert(), Some(("value".to_owned(), 0..5)));
}

#[test]
fn layer_expression_commands_disable_convert_and_reveal_errors() {
    use crate::actions::ids;
    let mut h = H::new();
    let layer = h.layer.clone();
    h.run(
        "attr.set_expression",
        json!({"path": layer, "attr": ROT, "source": "time * 30"}),
    );
    let mut tl = h.bars();
    let spec = |h: &H| query::attr_spec(&h.stage, &layer, ROT).cloned().unwrap();
    // Layer > Switches > Disable / Enable Expressions.
    h.session().lock().selected_layers = vec![layer.clone()];
    let r = h.action(&mut tl, Invocation::new(ids::DISABLE_EXPRESSIONS));
    h.apply(&r);
    assert!(!spec(&h).expression.unwrap().enabled);
    let r = h.action(&mut tl, Invocation::new(ids::ENABLE_EXPRESSIONS));
    h.apply(&r);
    assert!(spec(&h).expression.unwrap().enabled);
    // Convert Expression to Keyframes: a key per frame, the expression off.
    h.session().lock().selected_props = vec![(layer.clone(), ROT.to_owned())];
    let r = h.action(&mut tl, Invocation::new(ids::CONVERT_EXPRESSION));
    h.apply(&r);
    let s = spec(&h);
    assert!(!s.expression.as_ref().unwrap().enabled);
    let k = s.keyframes.get(&Time::from_seconds(1.0)).unwrap();
    assert_eq!(k.value, Value::Float(30.0));
    assert!(s.keyframes.len() > 30, "{}", s.keyframes.len());
    h.undo();
    assert!(spec(&h).keyframes.is_empty());
    // Reveal Expression Errors: exactly the failing properties.
    h.session().lock().expr_errors = vec![bae_anim::AttrError {
        prim: layer.clone(),
        attr: "transform:opacity".into(),
        message: "ReferenceError".into(),
    }];
    h.action(&mut tl, Invocation::new(ids::REVEAL_EXPRESSION_ERRORS));
    assert!(matches!(
        h.session().lock().reveal.get(&layer),
        Some(crate::session::Reveal::Props(p)) if p == &[(layer.clone(), "transform:opacity".to_owned())]
    ));
}

/// #592: an expression just added takes the focus with its text selected, so
/// typing replaces the default (AE).
#[test]
fn added_expression_is_focused_with_its_text_selected() {
    use crate::actions::ids;
    let mut h = H::new();
    let layer = h.layer.clone();
    h.session().lock().selected_props = vec![(layer.clone(), POS.to_owned())];
    let mut tl = h.bars();
    let r = h.action(&mut tl, Invocation::new(ids::ADD_EXPRESSION));
    h.apply(&r);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![Event::Text("wiggle(2, 50)".into())],
        Modifiers::NONE,
    );
    bae_ui::set_keypad_enter(true);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    bae_ui::set_keypad_enter(false);
    h.apply(&r);
    let source = query::attr_spec(&h.stage, &layer, POS)
        .and_then(|s| s.expression.clone())
        .map(|e| e.source);
    assert_eq!(source.as_deref(), Some("wiggle(2, 50)"));
}
