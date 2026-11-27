use super::*;

#[test]
fn marker_types_and_project_folders_take_their_configured_labels() {
    let mut h = gui();
    new_comp(&mut h);
    new_solid(&mut h);
    let layer = h.state().layers()[0].clone();
    h.state_mut().workspace.focus_panel("timeline");
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("labels.label_default_marker_comment", 3);
    prefs.set("labels.label_default_marker_segmentation", 9);
    prefs.set("labels.label_default_folder", 12);
    prefs.set("labels.label_name_3", "Editorial");
    prefs.set("labels.label_name_9", "Cut");
    h.state().prefs.set(prefs).unwrap();
    h.state_mut()
        .invoke(Invocation::new("motion.timeline.add_marker_with_dialog"));
    settle(&mut h);
    assert!(find_text(&h, "Editorial").is_some());
    let segment = find_text(&h, "Segment Marker").unwrap();
    click_at(&mut h, segment.center());
    settle(&mut h);
    assert!(find_text(&h, "Cut").is_some());
    key(&mut h, Modifiers::NONE, Key::Enter);
    settle(&mut h);
    let label = |h: &Ui, path: &PrimPath| {
        h.state()
            .stage()
            .prim(path)
            .and_then(|v| bae_motion::query::static_value(&v, "label"))
            .and_then(|v| v.as_f64())
    };
    let markers = bae_motion::markers::markers_of(h.state().stage(), &layer);
    assert_eq!(markers.len(), 1);
    assert_eq!(label(&h, &markers[0].0), Some(9.0));
    h.state_mut()
        .invoke(Invocation::new("motion.file.new_folder"));
    settle(&mut h);
    assert!(
        h.state()
            .stage()
            .children(&PrimPath::root())
            .iter()
            .any(|p| label(&h, p) == Some(12.0))
    );
}
