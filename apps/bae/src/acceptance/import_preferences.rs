use super::*;
use bae_ui::files::{FilePicker, ImportSelection, OpenRequest, SharedFilePicker};

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
