use super::*;
use bae_ui::files::{FilePicker, ImportSelection, OpenRequest, SharedFilePicker};

/// Import ▸ Enable Hardware Accelerated Decoding (Requires Restart): OK
/// keeps the switch; the next launch reads it (`startup::run`).
#[cfg(windows)]
#[test]
fn import_dialog_saves_the_hardware_decoding_switch() {
    use egui_kittest::kittest::Queryable as _;

    let mut h = gui();
    let id = "import.enable_hardware_accelerated_decoding";
    assert!(h.state().prefs.current().bool(id));
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "import"}),
    ));
    settle(&mut h);
    h.get_by_label("Enable Hardware Accelerated Decoding (Requires Restart)")
        .click();
    settle(&mut h);
    h.get_by_label("OK").click();
    settle(&mut h);
    assert!(!h.state().prefs.current().bool(id));
}

struct ImportPicker {
    first: PathBuf,
    alphabetical: bool,
    photoshop_kind: bae_model::psd::ImportKind,
}
impl FilePicker for ImportPicker {
    fn open(&self, _: &OpenRequest) -> Vec<PathBuf> {
        vec![self.first.clone()]
    }
    fn import(&self, request: &OpenRequest) -> ImportSelection {
        ImportSelection {
            paths: self.open(request),
            sequences: true,
            alphabetical: self.alphabetical,
            create_comp: false,
            photoshop_kind: self.photoshop_kind,
        }
    }
}

const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15, 0, 2,
    135, 1, 128, 235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn sequence_import_reports_missing_and_unreadable_frames_separately() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("镜头_001.png");
    std::fs::write(&first, PNG).unwrap();
    std::fs::write(dir.path().join("镜头_004.png"), &PNG[..45]).unwrap();
    std::fs::write(dir.path().join("镜头_005.png"), PNG).unwrap();
    for (report, verify, alphabetical, expected) in [
        (false, false, false, None),
        (true, false, false, Some("2–3")),
        (false, true, false, Some("4")),
        (false, true, true, Some("1")),
    ] {
        let mut h = gui();
        h.state_mut()
            .app
            .insert_resource(SharedFilePicker::new(ImportPicker {
                first: first.clone(),
                alphabetical,
                photoshop_kind: Default::default(),
            }));
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
            .push(std::sync::Arc::new(DroppedFile(file.clone())));
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
        settle(&mut h);
        h.get_by_label("OK").click();
        settle(&mut h);
        let stage = h.state().stage();
        let layers: Vec<_> = stage
            .traverse()
            .into_iter()
            .filter_map(|p| bae_model::psd::LayerImage::from_asset(&stage.prim(&p)?))
            .collect();
        assert!(
            layers
                .iter()
                .any(|l| l.key == bae_model::psd::LayerKey::Id(2) && l.canvas == canvas),
            "{mode}: {layers:?}, {:?}",
            painted_texts(&h)
        );
    }
}

#[test]
fn native_import_kind_reaches_the_layered_photoshop_import() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/bae-interop-psd/tests/fixtures/art.psd");
    let mut h = gui();
    h.state_mut()
        .app
        .insert_resource(SharedFilePicker::new(ImportPicker {
            first: path,
            alphabetical: false,
            photoshop_kind: bae_model::psd::ImportKind::RetainSize,
        }));
    h.state_mut().workspace.focus_panel("project");
    h.state_mut()
        .dispatch(Invocation::new("motion.file.import_file"));
    let selected_kind = bae_ui::tr("Composition - Retain Layer Sizes");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while find_text(&h, selected_kind).is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "selected import kind absent"
        );
        frame(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    key(&mut h, Modifiers::NONE, Key::Enter);
    settle(&mut h);
    assert!(!bae_motion::query::comps(h.state().stage()).is_empty());
}

#[test]
fn footage_replacement_reuses_or_resets_interpretation_and_ntsc_display() {
    let dir = tempfile::tempdir().unwrap();
    for stem in ["Plate", "Replacement"] {
        for i in 1..=2 {
            std::fs::write(dir.path().join(format!("{stem}_{i:03}.png")), PNG).unwrap();
        }
    }
    for (mode, drop, expected_rate) in [("remember", true, 12.0), ("reset", false, 29.97)] {
        let mut h = gui();
        let pick = |h: &mut Ui, name: &str| {
            h.state_mut()
                .app
                .insert_resource(SharedFilePicker::new(ImportPicker {
                    first: dir.path().join(name),
                    alphabetical: false,
                    photoshop_kind: Default::default(),
                }));
            h.state_mut().workspace.focus_panel("project");
        };
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("import.sequence_footage_fps", 29.97);
        prefs.set("import.footage_interpretation_on_replace", mode);
        prefs.set(
            "import.indeterminate_media_ntsc",
            if drop { "drop_frame" } else { "non_drop_frame" },
        );
        h.state().prefs.set(prefs).unwrap();
        pick(&mut h, "Plate_001.png");
        h.state_mut()
            .dispatch(Invocation::new("motion.file.import_file"));
        settle(&mut h);
        let item = h
            .state()
            .stage()
            .children(&PrimPath::new(bae_motion::query::ASSETS_FOLDER).unwrap())
            .into_iter()
            .find(|p| {
                h.state()
                    .stage()
                    .prim(p)
                    .is_some_and(|v| v.is_a(&bae_model::PrimType::ASSET))
            })
            .unwrap();
        h.state().session().lock().footage_view = Some(item.clone());
        h.state_mut().dispatch(Invocation::with_params(
            bae_ui::action::ids::OPEN_PANEL,
            serde_json::json!({"id":"footage"}),
        ));
        settle(&mut h);
        assert!(find_text(&h, if drop { "0;00;00;00" } else { "0:00:00:00" }).is_some());
        h.state_mut().dispatch(Invocation::with_params("footage.interpret", serde_json::json!({"items":[item],"frame_rate":12,"loop_count":4,"alpha_mode":"ignore"})));
        h.state().session().lock().selected_items = vec![item.clone()];
        pick(&mut h, "Replacement_001.png");
        h.state_mut()
            .dispatch(Invocation::new("motion.file.replace_footage_file"));
        settle(&mut h);
        let asset = h.state().stage().prim(&item).unwrap();
        assert!(
            bae_model::AssetFile::from_prim(&asset)
                .unwrap()
                .original
                .ends_with("Replacement_001.png")
        );
        let Some(bae_model::Value::FrameRate(rate)) = asset.value("frame_rate") else {
            panic!("missing rate")
        };
        assert!((rate.as_f64() - expected_rate).abs() < 0.001, "{rate:?}");
        assert_eq!(
            asset.value("loop").and_then(|v| v.as_f64()).unwrap_or(1.0),
            if mode == "remember" { 4. } else { 1. }
        );
        assert_eq!(
            asset.value("alpha_mode").and_then(|v| v.as_str()),
            Some(if mode == "remember" {
                "ignore"
            } else {
                "straight"
            })
        );
    }
}

#[test]
fn media_cache_folders_and_clean_button_use_the_disk_page_draft() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page":"disk"}),
    ));
    settle(&mut h);
    for (button, key, folder) in [
        (
            "Database: Choose Folder...",
            "disk.media_cache_database_folder",
            "index",
        ),
        (
            "Cache: Choose Folder...",
            "disk.media_cache_folder",
            "audio",
        ),
    ] {
        h.get_by_label(button).click();
        settle(&mut h);
        assert_eq!(
            h.state_mut().workspace.take_settings_requests(),
            [format!("choose:{key}")]
        );
        h.state_mut()
            .workspace
            .set_setting(key, dir.path().join(folder).to_string_lossy().into_owned());
        settle(&mut h);
    }
    h.get_by_label("Clean Database & Cache").click();
    settle(&mut h);
    assert_eq!(
        h.state_mut().workspace.take_settings_requests(),
        ["disk.clean_database_and_cache"]
    );
    h.get_by_label("OK").click();
    settle(&mut h);
    assert_eq!(
        Path::new(h.state().prefs.current().str("disk.media_cache_folder")),
        dir.path().join("audio")
    );
    assert_eq!(
        Path::new(
            h.state()
                .prefs
                .current()
                .str("disk.media_cache_database_folder")
        ),
        dir.path().join("index")
    );
}

#[test]
fn returning_to_the_app_reloads_only_the_selected_footage_types() {
    use std::io::Write;
    use std::time::{Duration, Instant};
    for mode in ["off", "non_sequence_footage", "all_footage_types"] {
        let dir = tempfile::tempdir().unwrap();
        let still = dir.path().join("Still.png");
        let first = dir.path().join("Shot_001.png");
        let next = dir.path().join("Shot_002.png");
        for p in [&still, &first, &next] {
            std::fs::write(p, PNG).unwrap();
        }
        let mut h = gui();
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("import.auto_reload_mode", mode);
        h.state().prefs.set(prefs).unwrap();
        for p in [&still, &first] {
            h.state_mut()
                .app
                .insert_resource(SharedFilePicker::new(ImportPicker {
                    first: p.clone(),
                    alphabetical: false,
                    photoshop_kind: Default::default(),
                }));
            h.state_mut().workspace.focus_panel("project");
            h.state_mut()
                .dispatch(Invocation::new("motion.file.import_file"));
            settle(&mut h);
        }
        let focus = |h: &mut Ui, active| {
            h.input_mut().focused = active;
            send(h, Event::WindowFocused(active));
        };
        let append = |p: &Path| {
            std::fs::OpenOptions::new()
                .append(true)
                .open(p)
                .unwrap()
                .write_all(&[0])
                .unwrap();
        };
        let size = |h: &Ui, path: &Path| {
            h.state()
                .stage()
                .traverse()
                .into_iter()
                .filter_map(|item| {
                    let view = h.state().stage().prim(&item)?;
                    bae_model::AssetFile::from_prim(&view)
                })
                .find(|f| Path::new(&f.original).file_name() == path.file_name())
                .unwrap()
                .size
        };
        let wait_for = |h: &mut Ui, check: &dyn Fn(&Ui) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(6);
            while !check(h) {
                assert!(Instant::now() < deadline, "focus scan for {mode}");
                frame(h);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        focus(&mut h, false);
        append(&still);
        append(&first);
        focus(&mut h, true);
        let original = PNG.len() as u64;
        if mode != "off" {
            wait_for(&mut h, &|h| size(h, &still) == original + 1);
        }
        if mode == "all_footage_types" {
            wait_for(&mut h, &|h| size(h, &first) == original + 1);
            // Both files came back in one step.
            key(&mut h, Modifiers::COMMAND, Key::Z);
            settle(&mut h);
            assert_eq!((size(&h, &still), size(&h, &first)), (original, original));
            key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
            settle(&mut h);
            h.state_mut().dispatched.clear();
            focus(&mut h, false);
            append(&next);
            focus(&mut h, true);
            wait_for(&mut h, &|h| {
                h.state()
                    .dispatched
                    .iter()
                    .any(|i| i.id == "item.reload_files")
            });
        } else {
            settle(&mut h);
            assert_eq!(size(&h, &first), original);
            assert_eq!(size(&h, &still), original + u64::from(mode != "off"));
        }
    }
}
