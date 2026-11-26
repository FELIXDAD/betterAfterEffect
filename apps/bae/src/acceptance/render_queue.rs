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
            "shape:ellipse",
            "motion.layer.new.text",
        ],
    );
    // Cmd+M queues the active comp and opens the Render Queue.
    key(&mut h, Modifiers::COMMAND, Key::M);
    assert_eq!(items(&h).len(), 1);
    let first = &items(&h)[0];
    if cfg!(target_os = "macos") {
        assert_eq!(first.format, ExportFormat::H264);
        assert!(first.output.ends_with("Comp 1.mp4"));
    } else {
        assert_eq!(first.format, ExportFormat::Png);
        assert!(first.output.ends_with("Comp 1_[####].png"));
    }
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
        wait_done(&mut h, Duration::from_secs(120));
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
            queue_control(&h, 1, "Show in Finder").is_some(),
            "{ext}: no Show in Finder"
        );
        queue_control(&h, 1, "Render Again")
            .expect("Render Again")
            .click();
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
    settle(h);
    confirm_dialog(h);
    assert!(add_layer(h, "shape:ellipse"));
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
            started.elapsed() < Duration::from_secs(120),
            "{:?}",
            items(&h)[0].status
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        h.get_by_label("Render").accesskit_node().is_disabled(),
        "Render enabled while rendering"
    );
    queue_control(&h, 1, "Stop").expect("Stop").click();
    settle(&mut h);
    wait_done(&mut h, Duration::from_secs(120));
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
        wait_done(&mut h, Duration::from_secs(120));
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
    wait_done(&mut h, Duration::from_secs(120));
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
    wait_done(&mut h, Duration::from_secs(120));
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
    wait_done(&mut h, Duration::from_secs(120));
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
    new_comp(&mut h);
    assert!(h.state_mut().menu("motion.layer.new.solid"));
    settle(&mut h);
    confirm_dialog(&mut h);
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

    // Edit right after starting: five new solids, then one undo. They run no
    // matter how far the render got (the expected layer count below must not
    // depend on how fast this machine renders); the 120 4K frames normally
    // outlast them, so they land while the worker renders its snapshot.
    let started = Instant::now();
    for _ in 0..5 {
        new_solid(&mut h);
    }
    key(&mut h, Modifiers::COMMAND, Key::Z);
    let edits_took = started.elapsed();
    let rendering_after_edits = h.state().render_queue().lock().is_running();
    wait_done(&mut h, Duration::from_secs(300));
    println!(
        "render of 3 items: {:?}; the 6 edits took {edits_took:?}, render still running \
         after them: {rendering_after_edits}",
        started.elapsed()
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
    wait_done(&mut h, Duration::from_secs(120));
    queue_control(&h, 1, "Show in Finder")
        .expect("Show in Finder")
        .click();
    settle(&mut h);
    // `reveal` spawns `open` without waiting for it: wait for the fake's
    // record, which appears in one rename (see `in_child_with_fake_open`).
    let log = PathBuf::from(std::env::var("BAE_ACC_OPEN_LOG").unwrap());
    let started = Instant::now();
    while !log.exists() && started.elapsed() < Duration::from_secs(120) {
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
        // Written to a temporary file and renamed into place, so the record
        // appears complete or not at all (a plain `> log` creates the file
        // before the arguments are in it: the test could read it empty).
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"$BAE_ACC_OPEN_LOG.tmp\" && mv \"$BAE_ACC_OPEN_LOG.tmp\" \"$BAE_ACC_OPEN_LOG\"\n",
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
        comp_with(&mut h, &["motion.layer.new.solid", "shape:ellipse"]);
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
        wait_done(&mut h, Duration::from_secs(120));
        let status = items(&h)[0].status.clone();
        let files = files_in(&volume, "tif");
        println!("FULL DISK: {status:?}; files {files:?}");
        assert!(matches!(status, ItemStatus::Failed(_)), "{status:?}");
        // The app keeps working.
        new_solid(&mut h);
    }));
    let _ = std::process::Command::new("hdiutil")
        .args(["detach", &dev])
        .output();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

// ---- Retest of #110 (dev 763eaad): edge cases ----

/// Queue the active comp once more and set the new item's output.
fn queue_one(h: &mut Ui, format: ExportFormat, output: &Path, start: i64, end: i64) -> usize {
    key(h, Modifiers::COMMAND, Key::M);
    let i = items(h).len() - 1;
    let queue = h.state().render_queue();
    let mut q = queue.lock();
    let id = q.items()[i].id;
    let item = q.item_mut(id).unwrap();
    item.format = format;
    item.output = output.to_string_lossy().into_owned();
    item.start = start;
    item.end = end;
    i
}

/// Render and collect every `Rendering { done, total }` the panel showed.
fn render_watching_progress(h: &mut Ui, i: usize) -> std::collections::BTreeSet<(usize, usize)> {
    render(h);
    let mut seen = std::collections::BTreeSet::new();
    let started = Instant::now();
    loop {
        frame(h);
        if let ItemStatus::Rendering { done, total } = items(h)[i].status {
            seen.insert((done, total));
        }
        if !h.state().render_queue().lock().is_running() {
            frame(h);
            return seen;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "{:?}",
            items(h)[i].status
        );
    }
}

#[test]
fn whitespace_output_is_refused_with_a_visible_message() {
    let Some(mut h) = gpu_gui() else { return };
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let i = queue_one(&mut h, ExportFormat::Png, Path::new("   "), 0, 2);
    render_watching_progress(&mut h, i);
    let ItemStatus::Failed(message) = items(&h)[i].status.clone() else {
        panic!("{:?}", items(&h)[i].status);
    };
    assert_eq!(message, "Choose where to render: Output To is empty.");
    assert!(
        find_text(&h, &message).is_some(),
        "message not shown in the panel"
    );
}

#[test]
fn ranges_are_limited_to_the_comp() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let i = queue_one(
        &mut h,
        ExportFormat::Png,
        &tmp.path().join("neg_[####].png"),
        -5,
        2,
    );
    render_watching_progress(&mut h, i);
    assert!(
        matches!(items(&h)[i].status, ItemStatus::Done { files: 3, .. }),
        "{:?}",
        items(&h)[i].status
    );
    assert_eq!(
        files_in(tmp.path(), "png").first().map(String::as_str),
        Some("neg_0000.png")
    );
    let i = queue_one(
        &mut h,
        ExportFormat::Png,
        &tmp.path().join("out_[####].png"),
        400,
        410,
    );
    render_watching_progress(&mut h, i);
    assert_eq!(
        items(&h)[i].status,
        ItemStatus::Failed("The frames are outside the composition (0–299).".into())
    );
}

#[test]
fn progress_of_a_limited_range_counts_the_frames_it_renders() {
    // 1780–1900 on an 1800-frame comp renders 20 frames; the progress must
    // say “x / 20”, not “x / 121” (or the fields must show the limited range),
    // from the moment the item starts (“0 / 20”, issue #157) to the end.
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    long_comp(&mut h); // 1 800 frames, slow enough to see the progress
    let i = queue_one(
        &mut h,
        ExportFormat::Png,
        &tmp.path().join("end_[####].png"),
        1780,
        1900,
    );
    let seen = render_watching_progress(&mut h, i);
    assert!(
        matches!(items(&h)[i].status, ItemStatus::Done { files: 20, .. }),
        "{:?}",
        items(&h)[i].status
    );
    let item = items(&h)[i].clone();
    // Every state counts, the first “0 / x” included: the worker limits the
    // range when it takes the item (issue #157).
    let totals: std::collections::BTreeSet<usize> = seen.iter().map(|(_, total)| *total).collect();
    println!("progress seen: {seen:?}");
    assert!(
        totals.iter().all(|t| *t == 20) || (item.start, item.end) == (1780, 1799),
        "progress totals {totals:?} for 20 rendered frames; fields still {}–{}",
        item.start,
        item.end
    );
}

#[test]
fn a_folder_as_output_renders_into_the_folder() {
    // “…/输出/” (or an existing folder) names a folder: the frames belong in it,
    // not next to it as “输出_0000.png”.
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let existing = tmp.path().join("已有 目录");
    fs::create_dir(&existing).unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let slash = PathBuf::from(format!("{}/输出/", tmp.path().display()));
    for out in [slash, existing.clone()] {
        let i = queue_one(&mut h, ExportFormat::Png, &out, 0, 2);
        render_watching_progress(&mut h, i);
        let inside = files_in(&tmp.path().join(out.file_name().unwrap()), "png");
        let beside = files_in(tmp.path(), "png");
        assert!(
            inside.len() == 3 && beside.is_empty(),
            "{}: inside {inside:?}, beside {beside:?}",
            out.display()
        );
    }
}

#[test]
fn the_file_extension_matches_the_format() {
    // A typed “.png” name with TIFF chosen writes TIFF data into .png files.
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let i = queue_one(
        &mut h,
        ExportFormat::Tiff,
        &tmp.path().join("错配_[####].png"),
        0,
        1,
    );
    render_watching_progress(&mut h, i);
    for name in fs::read_dir(tmp.path()).unwrap().map(|e| e.unwrap().path()) {
        let bytes = fs::read(&name).unwrap();
        let is_tiff = bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*");
        let ext = name
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        assert!(
            !is_tiff || ext == "tif" || ext == "tiff",
            "{} holds TIFF data",
            name.display()
        );
    }
}

// ---- Retest of #110 (dev c20770a): edge cases ----

#[test]
fn chinese_folders_get_the_comp_name_inside() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    // A comp name with characters a file name cannot hold.
    let comp = h.state().active_comp().unwrap();
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        json!({"path": comp, "attr": "name", "value": {"type": "string", "value": "片头/镜头:01"}}),
    ));
    settle(&mut h);
    let existing = tmp.path().join("已有 输出 文件夹");
    fs::create_dir(&existing).unwrap();
    let cases = [
        ("existing folder", existing.clone(), existing.clone()),
        (
            "new folder with a slash",
            PathBuf::from(format!("{}/新的 文件夹/", tmp.path().display())),
            tmp.path().join("新的 文件夹"),
        ),
    ];
    for (label, out, dir) in cases {
        let i = queue_one(&mut h, ExportFormat::Png, &out, 0, 2);
        render_watching_progress(&mut h, i);
        let status = items(&h)[i].status.clone();
        let files = files_in(&dir, "png");
        println!("{label}: {status:?} {files:?}");
        assert!(
            matches!(status, ItemStatus::Done { files: 3, .. }),
            "{label}: {status:?}"
        );
        assert_eq!(files.len(), 3, "{label}: {files:?}");
        assert!(
            files[0].starts_with("片头_镜头_01_"),
            "{label}: name {:?}",
            files[0]
        );
    }
}

#[test]
fn switching_formats_back_and_forth_keeps_the_name_and_fixes_the_extension() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let i = queue_one(
        &mut h,
        ExportFormat::Png,
        &tmp.path().join("镜头 01_[####].png"),
        0,
        1,
    );
    let queue = h.state().render_queue();
    let output_after = |f: ExportFormat| {
        let mut q = queue.lock();
        let id = q.items()[i].id;
        let item = q.item_mut(id).unwrap();
        item.set_format(f);
        item.output.clone()
    };
    for (f, ext) in [
        (ExportFormat::Tiff, "tif"),
        (ExportFormat::Exr, "exr"),
        (ExportFormat::Png, "png"),
        (ExportFormat::Exr, "exr"),
    ] {
        let out = output_after(f);
        assert!(
            out.ends_with(&format!("镜头 01_[####].{ext}")),
            "{f:?}: {out}"
        );
    }
    render_watching_progress(&mut h, i);
    let files = files_in(tmp.path(), "exr");
    assert_eq!(files, ["镜头 01_0000.exr", "镜头 01_0001.exr"]);
    let magic = fs::read(tmp.path().join(&files[0])).unwrap();
    assert!(
        magic.starts_with(&[0x76, 0x2f, 0x31, 0x01]),
        "not an EXR file"
    );
    // A typed extension that does not match: the panel says what will happen,
    // and the file gets the format's extension.
    let j = queue_one(
        &mut h,
        ExportFormat::Tiff,
        &tmp.path().join("手写_[####].png"),
        0,
        0,
    );
    settle(&mut h);
    // The Render Queue shares the short Timeline frame (AE): maximize it (`)
    // to see the whole item.
    to_front(&mut h, "render_queue");
    h.state_mut()
        .dispatch(Invocation::new(bae_ui::action::ids::MAXIMIZE_FRAME));
    settle(&mut h);
    assert!(
        find_text(&h, "The file will be saved as .tif to match the format.").is_some(),
        "no hint for the mismatched extension"
    );
    render_watching_progress(&mut h, j);
    assert_eq!(files_in(tmp.path(), "tif"), ["手写_0000.tif"]);
    assert!(files_in(tmp.path(), "png").is_empty());
}

#[test]
fn typed_frame_numbers_stay_inside_the_comp() {
    let Some(mut h) = gpu_gui() else { return };
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    let fields = |h: &Ui| {
        ["Frames · Start", "Frames · End"].map(|name| {
            let node = queue_control(h, 1, name).expect("frame range control");
            (node.rect(), node.value().unwrap_or_default())
        })
    };
    let f = fields(&h);
    assert_eq!(
        f.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>(),
        ["0", "299"],
        "{f:?}"
    );
    let type_into = |h: &mut Ui, at: Pos2, text: &str| {
        pause(h);
        click_at(h, at);
        press(h, Modifiers::COMMAND, Key::A);
        send(h, Event::Text(text.to_owned()));
        key(h, Modifiers::NONE, Key::Enter);
    };
    type_into(&mut h, f[0].0.center(), "-25");
    type_into(&mut h, f[1].0.center(), "99999");
    let item = items(&h)[0].clone();
    assert_eq!(
        (item.start, item.end),
        (0, 299),
        "typed values not limited to the comp"
    );
    // Start after end: the end follows (or the start is limited to it).
    type_into(&mut h, f[0].0.center(), "250");
    let end_field = fields(&h)[1].0.center();
    type_into(&mut h, end_field, "100");
    let item = items(&h)[0].clone();
    assert!(
        item.start <= item.end,
        "start {} after end {}",
        item.start,
        item.end
    );
    println!("start/end after 250 then 100: {}–{}", item.start, item.end);
}

#[test]
fn a_single_frame_numbers_the_output_name() {
    // #207: start = end still numbers “[####]”; frame 5 then frame 6 keeps both.
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    for (format, ext) in [
        (ExportFormat::Png, "png"),
        (ExportFormat::Tiff, "tif"),
        (ExportFormat::Exr, "exr"),
    ] {
        for frame in [5, 6] {
            let out = tmp.path().join(format!("单帧_[####].{ext}"));
            let i = queue_one(&mut h, format, &out, frame, frame);
            render_watching_progress(&mut h, i);
        }
        let got = files_in(tmp.path(), ext);
        assert_eq!(
            got,
            [format!("单帧_0005.{ext}"), format!("单帧_0006.{ext}")],
            "{ext}"
        );
    }
}

// ---- Retest on dev 3d582ea: movies from the Render Queue (#239), #225, #231 ----

/// What an MP4 / QuickTime file holds, read from its boxes: the video codec
/// (sample entry type), its depth (32 = with alpha), the number of samples
/// (frames) and the duration in seconds. Enough for one video track.
fn movie_info(path: &Path) -> (String, u16, u32, f64) {
    let b = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let find = |tag: &[u8], from: usize| {
        b[from..]
            .windows(4)
            .position(|w| w == tag)
            .map(|i| i + from)
    };
    let u16_at = |i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_be_bytes(b[i..i + 4].try_into().unwrap());
    let u64_at = |i: usize| u64::from_be_bytes(b[i..i + 8].try_into().unwrap());
    let mvhd = find(b"mvhd", 0).expect("no mvhd");
    let (scale, duration) = if b[mvhd + 4] == 1 {
        (u32_at(mvhd + 24), u64_at(mvhd + 28))
    } else {
        (u32_at(mvhd + 16), u64::from(u32_at(mvhd + 20)))
    };
    let stsd = find(b"stsd", 0).expect("no stsd");
    let (codec, at) = [b"avc1", b"hvc1", b"hev1", b"apch", b"ap4h"]
        .iter()
        .find_map(|c| find(*c, stsd).map(|i| (String::from_utf8_lossy(*c).into_owned(), i)))
        .expect("no known video codec");
    let depth = u16_at(at + 78);
    let stsz = find(b"stsz", 0).expect("no stsz");
    (
        codec,
        depth,
        u32_at(stsz + 12),
        duration as f64 / f64::from(scale),
    )
}

/// Frames until item `i` has finished (Done, Failed or Stopped): a movie item
/// can still be Queued for a few frames after Render while its encoder starts,
/// when [`wait_done`] already sees the queue idle.
fn wait_item(h: &mut Ui, i: usize, timeout: Duration) -> ItemStatus {
    let started = Instant::now();
    loop {
        frame(h);
        let status = items(h)[i].status.clone();
        if matches!(
            status,
            ItemStatus::Done { .. } | ItemStatus::Failed(_) | ItemStatus::Stopped { .. }
        ) {
            settle(h);
            return status;
        }
        assert!(started.elapsed() < timeout, "item {i} still {status:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn set_channels(h: &Ui, index: usize, channels: bae_ui_motion::render_queue::Channels) {
    let queue = h.state().render_queue();
    let mut q = queue.lock();
    let id = q.items()[index].id;
    q.item_mut(id).unwrap().channels = channels;
}

#[test]
fn movies_from_the_render_queue_have_their_codec_frames_and_alpha() {
    use bae_ui_motion::render_queue::default_channels;
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky_dir(tmp.path());
    // A rectangle on a transparent background: the alpha shows in the file.
    comp_with(&mut h, &["shape:rect"]);
    let cases = [
        (ExportFormat::H264, "mp4", "avc1", false),
        (ExportFormat::Hevc, "mov", "hvc1", false),
        (ExportFormat::ProRes, "mov", "apch", false),
        (ExportFormat::ProRes4444, "mov", "ap4h", true),
    ];
    for (i, (format, ext, codec, alpha)) in cases.into_iter().enumerate() {
        key(&mut h, Modifiers::COMMAND, Key::M);
        let path = dir.join(format!("影片 {codec}.{ext}"));
        configure(&h, i, format, &path, 0, 59);
        // The output module's defaults for the format (as AE's templates).
        set_channels(&h, i, default_channels(format));
        settle(&mut h);
        render(&mut h);
        let status = wait_item(&mut h, i, Duration::from_secs(300));
        // The file the item wrote (BAE gives the format its own extension).
        let path = match &status {
            ItemStatus::Done { first, .. } => first.clone(),
            _ => path,
        };
        assert!(
            matches!(status, ItemStatus::Done { .. }),
            "{codec}: {status:?}"
        );
        let (got, depth, frames, seconds) = movie_info(&path);
        println!(
            "{codec}: {} → {got} depth {depth}, {frames} frames, {seconds:.3} s",
            path.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(got, codec);
        assert_eq!(frames, 60, "{codec}: frames");
        assert!((seconds - 2.0).abs() < 0.01, "{codec}: {seconds} s");
        assert_eq!(depth == 32, alpha, "{codec}: depth {depth}");
    }
}

#[test]
fn stopping_a_movie_keeps_a_playable_file_of_the_rendered_frames() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let path = tricky_dir(tmp.path()).join("停止 影片.mp4");
    long_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(&h, 0, ExportFormat::H264, &path, 0, 1799);
    settle(&mut h);
    render(&mut h);
    let started = Instant::now();
    loop {
        frame(&mut h);
        match items(&h)[0].status {
            ItemStatus::Rendering { done, .. } if done >= 20 => break,
            ItemStatus::Done { .. } | ItemStatus::Failed(_) | ItemStatus::Stopped { .. } => {
                panic!("finished before Stop: {:?}", items(&h)[0].status)
            }
            _ => {}
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "{:?}",
            items(&h)[0].status
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    queue_control(&h, 1, "Stop").expect("Stop").click();
    settle(&mut h);
    let ItemStatus::Stopped { done, total, .. } = wait_item(&mut h, 0, Duration::from_secs(120))
    else {
        panic!("not stopped: {:?}", items(&h)[0].status);
    };
    assert!(done > 0 && done < total, "{done}/{total}");
    if path.exists() {
        let (codec, _, frames, _) = movie_info(&path);
        println!("stopped at {done}/{total}: {codec}, {frames} frames in the file");
        assert_eq!(
            frames as usize, done,
            "frames in the stopped movie vs “Stopped (N/M)”"
        );
    } else {
        println!("stopped at {done}/{total}: no file left");
    }
}

#[test]
fn a_movie_into_a_read_only_folder_fails_with_a_message() {
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("只读 影片");
    fs::create_dir(&locked).unwrap();
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&locked, perms.clone()).unwrap();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(&h, 0, ExportFormat::ProRes, &locked.join("x.mov"), 0, 9);
    render(&mut h);
    let status = wait_item(&mut h, 0, Duration::from_secs(120));
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    fs::set_permissions(&locked, perms).unwrap();
    let ItemStatus::Failed(message) = status else {
        panic!("not failed: {status:?}");
    };
    println!("read-only folder: {message}");
    assert!(!message.is_empty());
    new_solid(&mut h); // the app keeps working
}

#[test]
#[ignore = "manual (macOS): creates and ejects a 4 MB RAM disk"]
fn a_movie_on_a_full_disk_fails_cleanly() {
    let Some(mut h) = gpu_gui() else { return };
    let dev = std::process::Command::new("hdiutil")
        .args(["attach", "-nomount", "ram://8192"])
        .output()
        .unwrap();
    let dev = String::from_utf8_lossy(&dev.stdout).trim().to_owned();
    assert!(dev.starts_with("/dev/disk"), "{dev}");
    let ok = std::process::Command::new("diskutil")
        .args(["erasevolume", "HFS+", "BAEFULLMOV", &dev])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let volume = PathBuf::from("/Volumes/BAEFULLMOV");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        long_comp(&mut h);
        key(&mut h, Modifiers::COMMAND, Key::M);
        configure(
            &h,
            0,
            ExportFormat::ProRes4444,
            &volume.join("满 盘.mov"),
            0,
            599,
        );
        render(&mut h);
        wait_done(&mut h, Duration::from_secs(300));
        let status = items(&h)[0].status.clone();
        println!("FULL DISK (movie): {status:?}");
        assert!(matches!(status, ItemStatus::Failed(_)), "{status:?}");
        new_solid(&mut h);
    }));
    let _ = std::process::Command::new("hdiutil")
        .args(["detach", &dev])
        .output();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn closing_the_project_while_rendering_stops_and_empties_the_queue() {
    // #231: the Render Queue belongs to the project (AE).
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("关闭 时 渲染");
    long_comp(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::M);
    configure(&h, 0, ExportFormat::Png, &out.join("f_[####].png"), 0, 1799);
    render(&mut h);
    let started = Instant::now();
    while !matches!(items(&h)[0].status, ItemStatus::Rendering { done, .. } if done >= 5) {
        frame(&mut h);
        assert!(started.elapsed() < Duration::from_secs(120));
        std::thread::sleep(Duration::from_millis(5));
    }
    let close = h
        .state()
        .menu_items()
        .into_iter()
        .find(|i| i.invocation.id == "file.close_project")
        .unwrap();
    h.state_mut()
        .dialogs
        .answers
        .push_back(crate::files::UnsavedChoice::DontSave);
    assert!(h.state_mut().menu(&close.id));
    settle(&mut h);
    assert!(
        items(&h).is_empty(),
        "queue after Close Project: {:?}",
        items(&h)
    );
    // The worker stops after the frames already in flight: the count on disk
    // settles quickly and stays far below the 1 800 frames of the range.
    let count = |h: &mut Ui| {
        for _ in 0..100 {
            frame(h);
            std::thread::sleep(Duration::from_millis(10));
        }
        files_in(&out, "png").len()
    };
    let at_close = files_in(&out, "png").len();
    let watch: Vec<usize> = (0..10).map(|_| count(&mut h)).collect();
    println!("frames on disk each second after Close Project: {at_close} {watch:?}");
    let (one, two, three) = (watch[0], watch[8], watch[9]);
    println!(
        "frames on disk after Close Project: {at_close}, +1 s {one}, +2 s {two}, +3 s {three}"
    );
    assert_eq!(
        two, three,
        "frames kept coming after Close Project: {at_close} → {one} → {two} → {three}"
    );
    assert!(three < 100, "{three} frames: the render did not stop");
    // The next project renders normally.
    comp_with(&mut h, &["motion.layer.new.solid"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    assert_eq!(items(&h).len(), 1);
    configure(
        &h,
        0,
        ExportFormat::Png,
        &tmp.path().join("新 工程/n_[####].png"),
        0,
        2,
    );
    settle(&mut h);
    render(&mut h);
    let status = wait_item(&mut h, 0, Duration::from_secs(120));
    assert!(matches!(status, ItemStatus::Done { .. }), "{status:?}");
}

#[test]
fn a_new_png_item_is_rgb_as_in_ae() {
    // #225: a new output module is RGB; the PNG has no alpha (colour type 2).
    let Some(mut h) = gpu_gui() else { return };
    let tmp = tempfile::tempdir().unwrap();
    comp_with(&mut h, &["shape:rect"]);
    key(&mut h, Modifiers::COMMAND, Key::M);
    assert_eq!(
        items(&h)[0].channels,
        bae_ui_motion::render_queue::Channels::Rgb
    );
    configure(
        &h,
        0,
        ExportFormat::Png,
        &tmp.path().join("rgb_[####].png"),
        0,
        0,
    );
    render(&mut h);
    wait_done(&mut h, Duration::from_secs(120));
    let png = fs::read(tmp.path().join("rgb_0000.png")).unwrap();
    // IHDR: colour type at byte 25 (2 = RGB, 6 = RGBA).
    assert_eq!(png[25], 2, "PNG colour type {}", png[25]);
}
