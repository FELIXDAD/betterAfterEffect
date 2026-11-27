use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn composition_and_timeline_tabs_follow_the_item_label() {
    let mut h = gui();
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("labels.label_color_7", "#03d181");
    h.state().prefs.set(prefs.clone()).unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        "comp.create",
        serde_json::json!({
            "name": "Tab colors", "label": 7
        }),
    ));
    let comp = bae_motion::query::comps(h.state().stage()).pop().unwrap();
    h.state().session().lock().open_comp(&comp);
    settle(&mut h);
    let shows_swatch = |h: &Ui, title: &str| {
        // The Timeline tab shows only the comp (AE); find its text in the tab.
        let text = match title.strip_prefix("Timeline ") {
            Some(comp) => {
                let tab = h
                    .query_all_by_label(title)
                    .next()
                    .unwrap_or_else(|| panic!("missing tab {title}"))
                    .rect();
                find_texts(h, |t| t == comp)
                    .into_iter()
                    .find(|r| tab.contains(r.center()))
            }
            None => find_text(h, title),
        }
        .unwrap_or_else(|| panic!("missing tab {title}"));
        h.output().shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgb(3,209,129)
                && (r.rect.center().y-text.center().y).abs() < 5.0
                && r.rect.right() <= text.left() && r.rect.right() >= text.left()-36.0)
        })
    };
    let gradient = |h: &Ui| {
        let title = find_text(h, "Composition Tab colors").unwrap();
        h.output().shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::Mesh(m)
            if m.vertices.len() == 4 && m.calc_bounds().contains(title.center())
            && m.vertices[0].color != m.vertices[2].color)
        })
    };
    assert!(gradient(&h));
    for name in ["Composition Tab colors", "Timeline Tab colors"] {
        assert!(shows_swatch(&h, name), "missing label on {name}");
    }
    prefs.set("appearance.use_label_color_for_related_tabs", false);
    prefs.set("appearance.use_gradients", false);
    h.state().prefs.set(prefs).unwrap();
    settle(&mut h);
    assert!(!gradient(&h));
    for name in ["Composition Tab colors", "Timeline Tab colors"] {
        assert!(!shows_swatch(&h, name), "label still shown on {name}");
    }
}
