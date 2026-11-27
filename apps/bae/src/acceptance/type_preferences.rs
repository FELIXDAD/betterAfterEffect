use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn character_menu_uses_preview_size_and_recent_font_count() {
    let mut fonts = bae_text::FontSystem::new();
    let families = fonts.families();
    let [_, second, third, fourth, ..] = families.as_slice() else {
        return;
    };
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::Num6);
    settle(&mut h);
    let open = |h: &mut Ui| {
        h.get_by_label("Set the font family").click();
        settle(h);
    };
    open(&mut h);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    while find_text(&h, second).is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "font picker did not populate"
        );
        settle(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    let gap = |h: &Ui| {
        (find_text(h, fourth).unwrap().center().y - find_text(h, third).unwrap().center().y).abs()
    };
    let preview = |h: &Ui, height: f32| {
        let y = find_text(h, third).unwrap().center().y;
        h.output().shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) => {
                let rect = mesh.calc_bounds();
                (rect.center().y - y).abs() < 2.0
                    && (rect.height() - height).abs() < 1.0
                    && matches!(mesh.texture_id, egui::TextureId::Managed(id) if id > 0)
            }
            _ => false,
        })
    };
    while !preview(&h, 20.0) {
        assert!(
            std::time::Instant::now() < deadline,
            "font samples did not appear"
        );
        settle(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    let small = gap(&h);
    h.get_by_label(second).click();
    settle(&mut h);
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("type.font_preview_size", "extra_large");
    prefs.set("type.recent_fonts_count", "1");
    h.state().prefs.set(prefs.clone()).unwrap();
    open(&mut h);
    assert!(find_text(&h, "Recent Fonts").is_some());
    assert!(find_texts(&h, |t| t == second).len() >= 2);
    key(&mut h, Modifiers::NONE, Key::Escape);
    prefs.set("type.recent_fonts_count", "0");
    h.state().prefs.set(prefs).unwrap();
    open(&mut h);
    assert!(find_text(&h, "Recent Fonts").is_none());
    assert!(gap(&h) > small + 10.0);
    assert!(preview(&h, 42.0));
}

#[test]
fn opening_a_project_reports_missing_fonts_only_when_enabled() {
    const ABSENT: &str = "BAE-Acceptance-Absent-Font";
    let mut h = gui();
    comp_with(&mut h, &[]);
    let comp = h.state().active_comp().unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        "text.create",
        serde_json::json!({"comp": comp, "text": "字幕", "font": ABSENT, "position": [12, 20]}),
    ));
    assert_eq!(h.state().layers().len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("字幕.bae");
    h.state_mut().dialogs.save.push_back(Some(path.clone()));
    h.state_mut().dispatch(Invocation::new(ids::SAVE_AS));
    assert!(path.join("project.json").is_file());
    let open = Invocation::with_params(ids::OPEN_PROJECT, serde_json::json!({"path": path}));
    h.state_mut().dispatch(open.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    while find_texts(&h, |s| s.contains(ABSENT)).is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "missing-font alert did not open"
        );
        settle(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(!h.state().doc.is_dirty());
    key(&mut h, Modifiers::NONE, Key::Enter);
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("type.do_not_warn_missing_fonts", true);
    h.state().prefs.set(prefs).unwrap();
    h.state_mut().dispatch(open);
    settle(&mut h);
    assert!(!dialog_open(&h));
    assert_eq!(h.state().layers().len(), 1);
}
