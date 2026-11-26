use super::*;
use bae_prefs::keys;
use egui_kittest::kittest::Queryable as _;

#[test]
fn starting_the_queue_saves_the_changed_project_when_enabled() {
    let mut app = Gui::new();
    if !app.with_gpu() {
        eprintln!("No GPU adapter for render queue auto-save acceptance");
        return;
    }
    let mut h = gui_with(app);
    new_comp(&mut h);
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("片头.bae");
    h.state_mut().doc.save_as(&project).unwrap();
    new_solid(&mut h);
    let layer = h.state().layers()[0].clone();
    let backup = tmp
        .path()
        .join(crate::autosave::FOLDER)
        .join("片头 auto-save 1.bae");
    for on in [false, true] {
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set(keys::AUTO_SAVE, false);
        prefs.set("project.auto_save_on_render_queue", on);
        h.state().prefs.set(prefs).unwrap();
        key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
        let queue = h.state().render_queue();
        {
            let mut q = queue.lock();
            let id = q.items().last().unwrap().id;
            let item = q.item_mut(id).unwrap();
            item.set_format(bae_render::ExportFormat::Png);
            item.output = tmp
                .path()
                .join(format!("frame-{on}.png"))
                .to_string_lossy()
                .into_owned();
            item.start = 0;
            item.end = 0;
        }
        h.get_by_label("Render").click();
        settle(&mut h);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while queue.lock().is_running() {
            assert!(std::time::Instant::now() < deadline);
            frame(&mut h);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(backup.join("project.json").is_file(), on);
    }
    let recovered = crate::document::Document::load(&backup).unwrap();
    assert!(recovered.project.stage.prim(&layer).is_some());
    assert_eq!(h.state().doc.location(), Some(project.as_path()));
    assert!(h.state().doc.is_dirty());
    eprintln!("Reopened the render-start backup with the unsaved layer present.");
}
