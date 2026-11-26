use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn memory_page_shows_machine_values_and_opens_both_reports() {
    let mut h = gui();
    let gpu_name = bae_gpu::GpuContext::new_headless(Default::default())
        .ok()
        .map(|gpu| {
            let name = gpu.adapter_info().name.clone();
            h.state_mut().app.insert_resource(gpu);
            name
        });
    let total = bae_platform::memory::total_bytes().expect("physical memory");
    let resident = bae_platform::memory::resident_bytes().expect("resident memory");
    assert!(resident > 0 && resident <= total);
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("memory_cpu.ram_reserved_for_other_apps_gb", 3);
    h.state().prefs.set(prefs).unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "memory_cpu"}),
    ));
    settle(&mut h);
    for n in [total, total.saturating_sub(3_u64 << 30)] {
        let text = format!("{:.1} GB", n as f64 / (1_u64 << 30) as f64);
        assert!(find_text(&h, &text).is_some(), "{text}");
    }
    for (button, title) in [
        ("Show Memory Usage Details...", "Memory Usage Details"),
        ("GPU Information...", "GPU Information"),
    ] {
        h.get_by_label(button).click();
        settle(&mut h);
        let requests = h.state_mut().workspace.take_settings_requests();
        assert_eq!(requests.len(), 1);
        let (shown, body) =
            crate::ui::hardware_information(&requests[0], h.state().app.resources()).unwrap();
        assert_eq!(shown, title);
        assert!(!body.is_empty());
        if title == "GPU Information"
            && let Some(name) = &gpu_name
        {
            assert!(body.contains(name) && body.contains("Maximum Texture Dimension:"));
        }
    }
}

#[test]
fn disk_page_measures_the_selected_volume_before_confirming_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = bae_platform::AppDirs::under_root(dir.path());
    let mut app = build_app();
    app.insert_resource(dirs.clone());
    let mut h = gui_with(Gui::with_app(app));
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page":"disk"}),
    ));
    settle(&mut h);
    h.get_by_label("Choose Folder...").click();
    settle(&mut h);
    assert_eq!(
        h.state_mut().workspace.take_settings_requests(),
        ["choose:disk.disk_cache_folder"]
    );
    h.state_mut().workspace.set_setting(
        "disk.disk_cache_folder",
        dir.path().to_string_lossy().into_owned(),
    );
    assert_eq!(
        h.state_mut().workspace.take_settings_requests(),
        ["disk.measure_free_space"]
    );
    let folder = h
        .state()
        .workspace
        .settings_draft()
        .preview_cache_dir(&dirs.cache);
    let free = bae_platform::disk::available_bytes(&folder).expect("cache volume");
    h.state_mut().workspace.cache_free_space(&folder, free);
    settle(&mut h);
    assert_eq!(
        h.state()
            .workspace
            .settings_draft()
            .int(bae_prefs::keys::DISK_LIMIT_GB),
        ((free / 10) >> 30).min(1024) as i64
    );
    h.get_by_label("OK").click();
    settle(&mut h);
    let p = h.state().prefs.current();
    assert_eq!(p.preview_cache_dir(&dirs.cache), folder);
    assert!(!p.to_json().contains_key(bae_prefs::keys::DISK_LIMIT_GB));
}
