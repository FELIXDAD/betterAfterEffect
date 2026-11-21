//! Render Queue through the panel: Cmd+M, PNG / TIFF / EXR, frame range,
//! paths with spaces and Chinese, Render / Stop (frames kept, N/M matches the
//! files on disk), invalid paths, a full disk, Show in Finder (the `open`
//! arguments), editing while it renders, several items. Needs a GPU; skipped
//! without one.

use std::fs;
use std::time::{Duration, Instant};

use bae_render::ExportFormat;
use bae_ui_motion::render_queue::{ItemStatus, QueueItem};
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use serde_json::json;

use super::*;

/// A GUI with the GPU, or `None` (test skipped).
fn gpu_gui() -> Option<Ui> {
    let mut g = Gui::new();
    if !g.with_gpu() {
        eprintln!("no GPU adapter: render queue test skipped");
        return None;
    }
    Some(gui_with(g))
}

fn items(h: &Ui) -> Vec<QueueItem> {
    h.state().render_queue().lock().items().to_vec()
}

fn configure(h: &Ui, index: usize, format: ExportFormat, output: &Path, start: i64, end: i64) {
    let queue = h.state().render_queue();
    let mut q = queue.lock();
    let id = q.items()[index].id;
    let item = q.item_mut(id).unwrap();
    item.set_format(format);
    item.output = output.to_string_lossy().into_owned();
    item.start = start;
    item.end = end;
}

/// Frames until no item renders (or `timeout`).
fn wait_done(h: &mut Ui, timeout: Duration) {
    let started = Instant::now();
    loop {
        frame(h);
        if !h.state().render_queue().lock().is_running() {
            frame(h);
            return;
        }
        assert!(
            started.elapsed() < timeout,
            "render did not finish: {:?}",
            items(h)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn files_in(dir: &Path, ext: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(ext))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn render(h: &mut Ui) {
    h.get_by_label("Render").click();
    settle(h);
}

#[test]
fn png_tiff_exr_with_ranges_into_tricky_paths() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    comp_with(
        &mut h,
        &[
            "motion.layer.new.solid",
            "motion.layer.new.ellipse",
            "motion.layer.new.text",
        ],
    );
    // Cmd+M queues the active comp and opens the Render Queue.
    key(&mut h, Modifiers::COMMAND, Key::M);
    assert_eq!(items(&h).len(), 1);
    let first = &items(&h)[0];
    assert_eq!(first.format, ExportFormat::Png);
    assert!(
        first.output.ends_with("Comp 1_[####].png"),
        "{}",
        first.output
    );
    assert_eq!(
        (first.start, first.end),
        (0, 299),
        "default range is the work area"
    );
    assert!(
        h.query_by_label("Render").is_some(),
        "Render Queue panel not open"
    );

    for (format, ext, range) in [
        (ExportFormat::Png, "png", (0, 9)),
        (ExportFormat::Tiff, "tif", (5, 9)),
        (ExportFormat::Exr, "exr", (290, 299)),
    ] {
        let out_dir = dir.join(format!("输出 {ext}"));
        configure(
            &h,
            0,
            format,
            &out_dir.join(format!("帧 序列_[####].{ext}")),
            range.0,
            range.1,
        );
        render(&mut h);
        wait_done(&mut h, Duration::from_secs(60));
        let item = items(&h)[0].clone();
        let n = (range.1 - range.0 + 1) as usize;
        match &item.status {
            ItemStatus::Done { files, first, .. } => {
                assert_eq!(*files, n, "{ext}: files reported");
                assert!(first.is_file(), "{ext}: first file {}", first.display());
            }
            other => panic!("{ext}: {other:?}"),
        }
        let written = files_in(&out_dir, ext);
        assert_eq!(written.len(), n, "{ext}: files on disk {written:?}");
        assert_eq!(
            written[0],
            format!("帧 序列_{:04}.{ext}", range.0),
            "{ext}: numbering"
        );
        // Done shows "Done · N · x s" and "Show in Finder".
        assert!(
            h.query_by_label("Show in Finder").is_some(),
            "{ext}: no Show in Finder"
        );
        h.get_by_label("Render Again").click();
        settle(&mut h);
    }
}

/// A 4K, 60 s comp (1 800 frames) so a render runs long enough to stop.
fn long_comp(h: &mut Ui) -> PrimPath {
    h.state_mut().invoke(Invocation::with_params(
        "comp.create",
        json!({"name": "Long 长", "width": 3840, "height": 2160, "fps": 30,
               "duration": 60 * bae_model::Time::FLICKS_PER_SECOND}),
    ));
    settle(h);
    let comp = bae_motion::query::comps(h.state().stage())
        .into_iter()
        .find(|c| h.state().name(c) == "Long 长")
        .expect("long comp");
    h.state().session().lock().open_comp(&comp);
    settle(h);
    assert!(h.state_mut().menu("motion.layer.new.solid"));
    assert!(h.state_mut().menu("motion.layer.new.ellipse"));
    settle(h);
    comp
}

#[test]
fn stop_keeps_the_rendered_frames_and_reports_n_of_m() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let out = tricky_dir(tmp.path()).join("停止 测试");
    long_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(&h, 0, ExportFormat::Png, &out.join("f_[####].png"), 0, 1799);
    render(&mut h);
    // Let some frames finish, then Stop.
    let started = Instant::now();
    loop {
        frame(&mut h);
        if let ItemStatus::Rendering { done, .. } = items(&h)[0].status
            && done >= 20
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "{:?}",
            items(&h)[0].status
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        h.get_by_label("Render").accesskit_node().is_disabled(),
        "Render enabled while rendering"
    );
    h.get_by_label("Stop").click();
    settle(&mut h);
    wait_done(&mut h, Duration::from_secs(60));
    let status = items(&h)[0].status.clone();
    let ItemStatus::Stopped { done, total, first } = status else {
        panic!("not stopped: {status:?}");
    };
    assert_eq!(total, 1800);
    assert!(done > 0 && done < total, "{done}/{total}");
    let on_disk = files_in(&out, "png");
    assert_eq!(on_disk.len(), done, "N in “Stopped (N/M)” vs files on disk");
    // The kept frames are complete, consecutive files from frame 0.
    for (i, name) in on_disk.iter().enumerate() {
        assert_eq!(*name, format!("f_{i:04}.png"));
        let bytes = fs::read(out.join(name)).unwrap();
        assert!(
            bytes.len() > 64 && bytes.ends_with(&[0xAE, 0x42, 0x60, 0x82]),
            "{name} truncated"
        );
    }
    assert!(first.is_some_and(|f| f.is_file()));
    let label = format!("Stopped ({done}/{total} frames)");
    assert!(
        find_text(&h, &label).is_some(),
        "panel does not show {label:?}"
    );
}

#[test]
fn invalid_output_paths_fail_with_a_message() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("只读");
    fs::create_dir(&locked).unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let file = tmp.path().join("a file");
    fs::write(&file, b"x").unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let cases: [(&str, PathBuf); 2] = [
        ("under a file", file.join("out_[####].png")),
        ("read-only folder", locked.join("out_[####].png")),
    ];
    let mut report = Vec::new();
    for (i, (label, path)) in cases.iter().enumerate() {
        key(&mut h, Modifiers::COMMAND, Key::M);
        configure(&h, i, ExportFormat::Png, path, 0, 2);
        render(&mut h);
        wait_done(&mut h, Duration::from_secs(30));
        let status = items(&h)[i].status.clone();
        report.push(format!("{label}: {status:?}"));
        match status {
            ItemStatus::Failed(message) => {
                assert!(!message.is_empty(), "{label}: empty message");
                assert!(
                    find_text(&h, &message).is_some(),
                    "{label}: message not shown"
                );
            }
            other => panic!("{label}: expected a failure, got {other:?}"),
        }
    }
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    println!("RENDER ERRORS\n{}", report.join("\n"));
}

#[test]
fn output_without_extension_gets_one() {
    // AE adds the format's extension to the output name.
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(
        &h,
        0,
        ExportFormat::Png,
        &tmp.path().join("noext_[####]"),
        0,
        2,
    );
    render(&mut h);
    wait_done(&mut h, Duration::from_secs(30));
    let all: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        files_in(tmp.path(), ".png").len(),
        3,
        "files written: {all:?}"
    );
}

#[test]
fn empty_output_path_is_refused() {
    // An empty "Output To" must not render into the process's working directory
    // (`/` for an app started from Finder, wherever the terminal was otherwise).
    let cwd = tempfile::tempdir().unwrap();
    if !in_child_at(
        "acceptance::render_queue::empty_output_path_is_refused",
        &[],
        Some(cwd.path()),
    ) {
        return;
    }
    let Some(mut h) = gpu_gui() else { return };
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(&h, 0, ExportFormat::Png, Path::new(""), 0, 2);
    render(&mut h);
    wait_done(&mut h, Duration::from_secs(30));
    let here = std::env::current_dir().unwrap();
    let stray = files_in(&here, ".png");
    let status = items(&h)[0].status.clone();
    assert!(
        matches!(status, ItemStatus::Failed(_)) && stray.is_empty(),
        "empty output: {status:?}, files in the working directory: {stray:?}"
    );
}

#[test]
fn frame_range_outside_the_comp() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    // The panel's frame fields accept any numbers; AE limits them to the comp.
    configure(
        &h,
        0,
        ExportFormat::Png,
        &tmp.path().join("r_[####].png"),
        298,
        305,
    );
    render(&mut h);
    wait_done(&mut h, Duration::from_secs(30));
    let written = files_in(tmp.path(), "png");
    println!(
        "range 298-305 of a 300-frame comp: {:?} -> {written:?}",
        items(&h)[0].status
    );
    assert!(
        written.len() <= 2,
        "frames past the end of the comp were rendered: {written:?}"
    );
}

#[test]
fn editing_while_rendering_and_several_items() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let comp = long_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(
        &h,
        0,
        ExportFormat::Png,
        &tmp.path().join("a").join("a_[####].png"),
        0,
        119,
    );
    // A second comp, queued twice with different ranges and formats.
    key(&mut h, Modifiers::COMMAND, Key::N);
    assert!(h.state_mut().menu("motion.layer.new.solid"));
    settle(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::M);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(
        &h,
        1,
        ExportFormat::Tiff,
        &tmp.path().join("b").join("b_[####].tif"),
        0,
        4,
    );
    configure(
        &h,
        2,
        ExportFormat::Exr,
        &tmp.path().join("c").join("c_[####].exr"),
        10,
        14,
    );
    assert_eq!(items(&h).len(), 3);
    render(&mut h);

    // Keep editing while it renders: new layers, a rename, undo; frames keep coming.
    let started = Instant::now();
    let mut ui_frames = 0;
    let mut edits = 0;
    while h.state().render_queue().lock().is_running() {
        if edits < 5 {
            key(&mut h, Modifiers::COMMAND, Key::Y);
            edits += 1;
        } else if edits == 5 {
            key(&mut h, Modifiers::COMMAND, Key::Z);
            edits += 1;
        }
        frame(&mut h);
        ui_frames += 1;
        assert!(
            started.elapsed() < Duration::from_secs(300),
            "{:?}",
            items(&h)
        );
    }
    frame(&mut h);
    let elapsed = started.elapsed();
    println!("render of 3 items: {elapsed:?}, {ui_frames} UI frames meanwhile");
    assert!(
        ui_frames > 10,
        "the UI did not keep running during the render"
    );
    for (i, (dir, ext, n)) in [("a", "png", 120), ("b", "tif", 5), ("c", "exr", 5)]
        .iter()
        .enumerate()
    {
        let status = &items(&h)[i].status;
        assert!(
            matches!(status, ItemStatus::Done { files, .. } if files == n),
            "{dir}: {status:?}"
        );
        assert_eq!(files_in(&tmp.path().join(dir), ext).len(), *n, "{dir}");
    }
    // The edits stayed in the project (render used a snapshot, nothing was lost).
    assert_eq!(
        h.state().layers().len(),
        1 + 4,
        "{:?}",
        h.state().layer_names()
    );
    assert!(h.state().stage().prim(&comp).is_some());
}

#[test]
fn show_in_finder_passes_the_path_to_open() {
    if !in_child_with_fake_open("acceptance::render_queue::show_in_finder_passes_the_path_to_open")
    {
        return;
    }
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let out = tricky_dir(tmp.path()).join("显示 在 访达");
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(&h, 0, ExportFormat::Png, &out.join("帧 _[####].png"), 0, 2);
    render(&mut h);
    wait_done(&mut h, Duration::from_secs(30));
    h.get_by_label("Show in Finder").click();
    settle(&mut h);
    let log = PathBuf::from(std::env::var("BAE_ACC_OPEN_LOG").unwrap());
    let started = Instant::now();
    while !log.exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let args: Vec<String> = fs::read_to_string(&log)
        .expect("`open` was not run")
        .lines()
        .map(str::to_owned)
        .collect();
    let first = out.join("帧 _0000.png");
    assert_eq!(
        args,
        ["-R".to_owned(), first.to_string_lossy().into_owned()]
    );
}

/// Like [`in_child`], with a fake `open` first on `PATH` that writes its
/// arguments (one per line) to `$BAE_ACC_OPEN_LOG` instead of opening Finder.
fn in_child_with_fake_open(test: &str) -> bool {
    if std::env::var_os("BAE_ACC_CHILD").is_some() {
        return true;
    }
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("open.log");
    let script = bin.path().join("open");
    fs::write(
        &script,
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"$BAE_ACC_OPEN_LOG\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    in_child(
        test,
        &[
            ("PATH", path.as_str()),
            ("BAE_ACC_OPEN_LOG", log.to_str().unwrap()),
        ],
    )
}

#[test]
#[ignore = "manual (macOS): creates and ejects a 4 MB RAM disk"]
fn full_disk_fails_cleanly() {
    let Some(mut h) = gpu_gui() else { return };
    let dev = std::process::Command::new("hdiutil")
        .args(["attach", "-nomount", "ram://8192"])
        .output()
        .unwrap();
    let dev = String::from_utf8_lossy(&dev.stdout).trim().to_owned();
    assert!(dev.starts_with("/dev/disk"), "{dev}");
    let ok = std::process::Command::new("diskutil")
        .args(["erasevolume", "HFS+", "BAEFULL", &dev])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let volume = PathBuf::from("/Volumes/BAEFULL");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        comp_with(
            &mut h,
            &["motion.layer.new.solid", "motion.layer.new.ellipse"],
        );
        key(&mut h, Modifiers::COMMAND, Key::M);
        configure(
            &h,
            0,
            ExportFormat::Tiff,
            &volume.join("满 盘_[####].tif"),
            0,
            29,
        );
        render(&mut h);
        wait_done(&mut h, Duration::from_secs(60));
        let status = items(&h)[0].status.clone();
        let files = files_in(&volume, "tif");
        println!("FULL DISK: {status:?}; files {files:?}");
        assert!(matches!(status, ItemStatus::Failed(_)), "{status:?}");
        // The app keeps working.
        key(&mut h, Modifiers::COMMAND, Key::Y);
    }));
    let _ = std::process::Command::new("hdiutil")
        .args(["detach", &dev])
        .output();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
