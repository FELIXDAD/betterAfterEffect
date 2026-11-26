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
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15,
        0, 2, 135, 1, 128, 235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
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
fn media_cache_folders_and_clean_button_use_the_disk_page_draft() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(bae_ui::settings::ACTION, serde_json::json!({"page":"disk"})));
    settle(&mut h);
    for (button,key,folder) in [("Database: Choose Folder...","disk.media_cache_database_folder","index"),("Cache: Choose Folder...","disk.media_cache_folder","audio")] {
        h.get_by_label(button).click();
        settle(&mut h);
        assert_eq!(h.state_mut().workspace.take_settings_requests(),[format!("choose:{key}")]);
        h.state_mut().workspace.set_setting(key,dir.path().join(folder).to_string_lossy().into_owned());
        settle(&mut h);
    }
    h.get_by_label("Clean Database & Cache").click();
    settle(&mut h);
    assert_eq!(h.state_mut().workspace.take_settings_requests(), ["disk.clean_database_and_cache"]);
    h.get_by_label("OK").click();
    settle(&mut h);
    assert_eq!(Path::new(h.state().prefs.current().str("disk.media_cache_folder")),dir.path().join("audio"));
    assert_eq!(Path::new(h.state().prefs.current().str("disk.media_cache_database_folder")),dir.path().join("index"));
}