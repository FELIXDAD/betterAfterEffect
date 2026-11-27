use super::*;
use bae_ui_motion::render_queue::ItemStatus;
use egui_kittest::kittest::Queryable as _;

#[test]
fn draft_and_lossless_alpha_render_a_half_size_transparent_png() {
    let mut app = Gui::new();
    if !app.with_gpu() {
        eprintln!("No GPU adapter for render queue acceptance");
        return;
    }
    let mut h = gui_with(app);
    new_comp(&mut h);
    key(&mut h, Modifiers::COMMAND | Modifiers::CTRL, Key::M);
    assert!(find_text(&h, "Queued").is_some());
    for (menu, preset) in [
        ("Render Settings Templates", "Draft Settings"),
        ("Output Module Templates", "Lossless with Alpha"),
    ] {
        queue_control(&h, 1, menu).unwrap().click();
        settle(&mut h);
        h.get_by_role_and_label(egui::accesskit::Role::Button, preset)
            .click();
        settle(&mut h);
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(find_text(&h, "Lossless with Alpha (PNG Sequence)").is_some());
    queue_control(&h, 1, "Output Module").unwrap().click();
    settle(&mut h);
    assert!(find_text(&h, "Image sequences do not contain audio.").is_some());
    h.get_by_role_and_label(egui::accesskit::Role::Button, "OK")
        .click();
    settle(&mut h);
    let queue = h.state().render_queue();
    let info = {
        let mut queue = queue.lock();
        let id = queue.items()[0].id;
        let item = queue.item_mut(id).unwrap();
        item.end = item.start;
        item.outputs[0].output = dir.path().join("draft_[####].png").display().to_string();
        bae_motion::query::comp_info(h.state().stage(), &item.comp).unwrap()
    };
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Render")
        .click();
    settle(&mut h);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while queue.lock().is_running() {
        assert!(std::time::Instant::now() < deadline, "render timed out");
        frame(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let status = queue.lock().items()[0].status.clone();
    settle(&mut h);
    assert!(find_text(&h, "Done").is_some());
    let ItemStatus::Done {
        files: 1, first, ..
    } = status
    else {
        panic!("unexpected queue status: {status:?}");
    };
    let (width, height, rgba) = bae_render::png_out::read_rgba8(&first).unwrap();
    assert_eq!(
        (width, height),
        (info.width.div_ceil(2), info.height.div_ceil(2))
    );
    assert_eq!(rgba[3], 0);
}
