use super::*;
use bae_media::still::{PngFormat, decode_still};
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
    for (menu, preset) in [
        ("Render Settings Templates", "Draft Settings"),
        ("Output Module Templates", "Lossless with Alpha"),
    ] {
        queue_control(&h, 1, menu).unwrap().click();
        settle(&mut h);
        h.get_by_role_and_label(egui::accesskit::Role::Button, preset).click();
        settle(&mut h);
    }
    let dir = tempfile::tempdir().unwrap();
    let queue = h.state().render_queue();
    let info = {
        let mut queue = queue.lock();
        let id = queue.items()[0].id;
        let item = queue.item_mut(id).unwrap();
        item.end = item.start;
        item.outputs[0].output = dir.path().join("draft_[####].png").display().to_string();
        bae_motion::query::comp_info(h.state().stage(), &item.comp).unwrap()
    };
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Render").click();
    settle(&mut h);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while queue.lock().is_running() {
        assert!(std::time::Instant::now() < deadline, "render timed out");
        frame(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let status = queue.lock().items()[0].status.clone();
    let ItemStatus::Done { files: 1, first, .. } = status else {
        panic!("unexpected queue status: {status:?}");
    };
    let frame = decode_still(&PngFormat, &first).unwrap();
    assert_eq!((frame.width, frame.height), (info.width.div_ceil(2), info.height.div_ceil(2)));
    assert_eq!(frame.data[3], 0);
}
