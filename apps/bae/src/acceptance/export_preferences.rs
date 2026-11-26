use super::*;
use bae_ui_motion::render_queue::ItemStatus;
use egui_kittest::kittest::Queryable as _;

#[test]
fn output_preferences_apply_to_new_items_and_segment_image_sequences() {
    let mut app = Gui::new();
    if !app.with_gpu() {
        eprintln!("No GPU adapter for render queue acceptance");
        return;
    }
    let mut h = gui_with(app);
    let tmp = tempfile::tempdir().unwrap();
    new_comp(&mut h);
    new_solid(&mut h);
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("export.use_default_filename_and_folder", false);
    prefs.set("export.segment_sequences_enabled", true);
    prefs.set("export.segment_sequences_files", 2);
    h.state().prefs.set(prefs).unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page":"memory_cpu"}),
    ));
    settle(&mut h);
    h.get_by_label("Enable Multi-Frame Rendering").click();
    settle(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "OK")
        .click();
    settle(&mut h);
    assert!(
        !h.state()
            .prefs
            .current()
            .bool("memory_cpu.enable_multi_frame_rendering")
    );
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    let queue = h.state().render_queue();
    {
        let mut q = queue.lock();
        assert_eq!(q.items().len(), 1);
        let id = q.items()[0].id;
        let item = q.item_mut(id).unwrap();
        assert!(item.outputs[0].output.is_empty());
        item.outputs[0].set_format(bae_render::ExportFormat::Png);
        item.outputs[0].output = tmp
            .path()
            .join("镜头_[###].png")
            .to_string_lossy()
            .into_owned();
        item.start = 7;
        item.end = 11;
    }
    h.get_by_role_and_label(egui::accesskit::Role::Button, bae_ui::tr("Render"))
        .click();
    settle(&mut h);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while queue.lock().is_running() {
        assert!(std::time::Instant::now() < deadline, "render timed out");
        frame(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let status = queue.lock().items()[0].status.clone();
    assert!(
        matches!(&status, ItemStatus::Done { files: 5, first, .. } if first.ends_with("镜头_0001/镜头_007.png")),
        "{status:?}"
    );
    for (folder, files) in [
        ("镜头_0001", vec!["镜头_007.png", "镜头_008.png"]),
        ("镜头_0002", vec!["镜头_009.png", "镜头_010.png"]),
        ("镜头_0003", vec!["镜头_011.png"]),
    ] {
        let paths = std::fs::read_dir(tmp.path().join(folder)).unwrap();
        let mut names: Vec<_> = paths
            .map(|p| p.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, files);
    }
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("export.use_default_filename_and_folder", true);
    h.state().prefs.set(prefs).unwrap();
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    let queue = queue.lock();
    let items = queue.items();
    let auto = Path::new(&items[1].outputs[0].output);
    let repeated = Path::new(&items[2].outputs[0].output);
    assert_eq!(auto.parent(), Some(tmp.path()));
    assert_eq!(repeated.parent(), Some(tmp.path()));
    assert_ne!(auto, repeated);
    assert!(
        repeated
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Comp 1_1")
    );
    eprintln!("Rendered frames 7–11 into three folders and verified unique queue output names.");
}

#[test]
fn info_and_flowchart_show_the_active_render_when_enabled() {
    let mut h = gui();
    new_comp(&mut h);
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    let queue = h.state().render_queue();
    {
        let mut q = queue.lock();
        let id = q.items()[0].id;
        q.item_mut(id).unwrap().status = ItemStatus::Rendering { done: 2, total: 12 };
    }
    for panel in ["info", "flowchart"] {
        to_front(&mut h, panel);
        for show in [true, false] {
            let mut prefs = (*h.state().prefs.current()).clone();
            prefs.set("appearance.show_rendering_progress_in_info_flowchart", show);
            h.state().prefs.set(prefs).unwrap();
            settle(&mut h);
            assert_eq!(
                find_text(&h, "Rendering Comp 1  2/12").is_some(),
                show,
                "{panel}"
            );
        }
    }
}
