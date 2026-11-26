const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15, 0, 2,
    135, 1, 128, 235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

use super::*;
use bae_ui::files::{FilePicker, OpenRequest, SharedFilePicker};

struct SequencePicker(PathBuf);
impl FilePicker for SequencePicker {
    fn open(&self, _: &OpenRequest) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }
}

#[test]
fn sequence_import_reports_missing_and_unreadable_frames_separately() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("镜头_001.png");
    std::fs::write(&first, PNG).unwrap();
    std::fs::write(dir.path().join("镜头_004.png"), &PNG[..45]).unwrap();
    std::fs::write(dir.path().join("镜头_005.png"), PNG).unwrap();
    for (report, verify, expected) in [
        (false, false, None),
        (true, false, Some("2–3")),
        (false, true, Some("4")),
    ] {
        let mut h = gui();
        h.state_mut()
            .app
            .insert_resource(SharedFilePicker::new(SequencePicker(first.clone())));
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("import.report_missing_frames", report);
        prefs.set("import.verify_individual_frames", verify);
        h.state().prefs.set(prefs).unwrap();
        h.state_mut().workspace.focus_panel("project");
        h.state_mut()
            .dispatch(Invocation::new("motion.file.import_file"));
        settle(&mut h);
        if let Some(frames) = expected {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !dialog_open(&h) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "sequence diagnostic missing"
                );
                frame(&mut h);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            settle(&mut h);
            let kind = if report {
                "Missing frames"
            } else {
                "Unreadable frames"
            };
            let message = format!("{}: {frames}", bae_ui::tr(kind));
            assert!(
                !find_texts(&h, |text| text.contains(&message)).is_empty(),
                "{message}"
            );
        } else {
            assert!(!dialog_open(&h));
        }
    }
}

#[test]
fn dropping_multiple_files_uses_the_selected_import_mode() {
    use bae_motion::query;

    let dir = tempfile::tempdir().unwrap();
    let files = ["left.png", "right.png"].map(|name| dir.path().join(name));
    for file in &files {
        std::fs::write(file, PNG).unwrap();
    }
    for (mode, nested) in [
        ("footage", false),
        ("composition", true),
        ("composition_retain_layer_sizes", true),
    ] {
        let mut h = gui();
        comp_with(&mut h, &[]);
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("import.drag_import_multiple_items_as", mode);
        h.state().prefs.set(prefs).unwrap();
        drop_on_canvas(&mut h, &files);
        let layers = h.state().layers();
        let stage = h.state().stage();
        let footage = if nested {
            assert_eq!(layers.len(), 1, "{mode}");
            let comp = query::precomp_source(stage, &layers[0]).expect(mode);
            query::comp_layers(stage, &comp)
        } else {
            layers
        };
        let mut names: Vec<_> = footage
            .iter()
            .map(|layer| query::display_name_of(stage, layer))
            .collect();
        names.sort();
        assert_eq!(names, ["left.png", "right.png"], "{mode}");
        key(&mut h, Modifiers::COMMAND, Key::Z);
        assert!(h.state().layers().is_empty(), "{mode}");
    }
}

fn drop_on_canvas(h: &mut Ui, files: &[PathBuf]) {
    use egui_kittest::kittest::Queryable as _;
    let viewer = h
        .root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap();
    send(h, Event::PointerMoved(viewer.center()));
    for file in files {
        h.input_mut()
            .dropped_files
            .push(std::sync::Arc::new(super::dialogs::DroppedFile(
                file.clone(),
            )));
    }
    settle(h);
}

#[test]
fn layered_drops_use_canvas_or_layer_size_from_preferences() {
    use egui_kittest::kittest::Queryable as _;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("overlay.png");
    std::fs::write(&file, PNG).unwrap();
    let psd = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/bae-interop-psd/tests/fixtures/art.psd");
    for (mode, canvas) in [
        ("composition", true),
        ("composition_retain_layer_sizes", false),
    ] {
        let mut h = gui();
        comp_with(&mut h, &[]);
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("import.drag_import_multiple_items_as", mode);
        h.state().prefs.set(prefs).unwrap();
        drop_on_canvas(&mut h, &[psd.clone(), file.clone()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while find_text(&h, "Editable Layer Styles").is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "PSD composition dialog missing for {mode}"
            );
            frame(&mut h);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.get_by_label("OK").click();
        settle(&mut h);
        let stage = h.state().stage();
        let layers: Vec<_> = stage
            .traverse()
            .into_iter()
            .filter_map(|p| bae_model::psd::LayerImage::from_asset(&stage.prim(&p)?))
            .collect();
        assert!(
            layers.iter().any(|l| l.canvas == canvas),
            "{mode}: {layers:?}"
        );
    }
}
