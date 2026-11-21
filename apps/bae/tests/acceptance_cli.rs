//! 0.0.1 acceptance (macOS run, 2026-11-21): the real `bae` executable.
//!
//! Version, `bae --headless <project>` with paths containing spaces and Chinese,
//! broken projects (clear message, exit 1, no crash), `bae export-json` equal to
//! the model's export, `bae render` into tricky paths, `bae mcp` over stdio
//! (create, animate, save, read back), and the headless smoke test with a
//! screenshot. macOS only (the suite of the macOS acceptance run).
#![cfg(target_os = "macos")]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use bae_model::{Project, Time};
use serde_json::{Value as Json, json};

const BAE: &str = env!("CARGO_BIN_EXE_bae");
const SEC: i64 = Time::FLICKS_PER_SECOND;

fn bae() -> Command {
    let mut c = Command::new(BAE);
    // Keep the user's settings, logs and crash reports out of the way.
    c.env("BAE_LOG", "warn");
    c
}

fn tricky(root: &Path) -> PathBuf {
    let dir = root.join("验收 测试 folder").join("命令 行");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project made with the motion commands: comp, solid, rectangle, two
/// position keyframes (one hold), a renamed layer.
fn make_project(path: &Path) -> Json {
    let mut builder = bae_app::AppBuilder::new();
    builder.add_module(bae_motion::MotionModule);
    let app = builder.build();
    let mut reg = bae_model::CommandRegistry::with_builtins();
    if let Some(r) = app.get_registry::<dyn bae_model::DynCommand>() {
        for e in r.snapshot().iter() {
            let _ = reg.register_dyn(e.item().clone());
        }
    }
    let mut project = Project::new();
    let author = bae_model::AuthorId::new("user:acceptance").unwrap();
    let mut run = |id: &str, p: Json| {
        reg.execute(&mut project.stage, &author, id, p)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    };
    let comp = run(
        "comp.create",
        json!({"name": "主 合成", "fps": 30, "duration": 2 * SEC, "width": 640, "height": 360}),
    );
    let comp = comp["path"].as_str().unwrap().to_owned();
    run(
        "layer.create",
        json!({"comp": comp, "kind": "solid", "name": "背景 Solid", "color": [0.2, 0.4, 0.8, 1.0]}),
    );
    let rect = run(
        "layer.create",
        json!({"comp": comp, "kind": "shape", "shape": "rect", "name": "Rect ✨"}),
    );
    let rect = rect["path"].as_str().unwrap().to_owned();
    run(
        "attr.set_animated",
        json!({"path": rect, "attr": "transform:position", "animated": true, "time": 0}),
    );
    run(
        "attr.set_at",
        json!({"path": rect, "attr": "transform:position", "time": SEC,
        "value": {"type": "vec3", "value": [500.0, 200.0, 0.0]}}),
    );
    project.save_as(path).unwrap();
    bae_model::export_flattened_json(&project.stage)
}

#[test]
fn version_flag() {
    let out = bae().arg("--version").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    println!("bae --version: {text}");
    assert!(text.starts_with("bae "), "{text}");
}

#[test]
fn headless_open_with_spaces_and_chinese() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    for name in ["工程 一.bae", "工程 二.baez"] {
        let path = dir.join(name);
        make_project(&path);
        let out = bae().arg("--headless").arg(&path).output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{name}: {stdout} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("opened") && stdout.contains(name),
            "{stdout}"
        );
    }
}

#[test]
fn broken_projects_exit_1_with_a_message() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let garbage = dir.join("坏的 文件.baez");
    std::fs::write(&garbage, b"PK\x03\x04 not really a zip").unwrap();
    let missing = dir.join("没有 这个.bae");
    for (path, args) in [
        (&garbage, vec!["--headless"]),
        (&missing, vec!["--headless"]),
    ] {
        let out = bae().args(&args).arg(path).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        println!(
            "{args:?} {}: exit {:?}: {stderr}",
            path.display(),
            out.status.code()
        );
        assert_eq!(out.status.code(), Some(1), "{args:?}: {stderr}");
        assert!(!stderr.contains("panicked"), "{args:?} panicked: {stderr}");
        let file = path.file_name().unwrap().to_string_lossy();
        assert!(
            stderr.contains(&*file),
            "{args:?}: message does not name the file: {stderr}"
        );
    }
}

#[test]
#[ignore = "issue #111"]
fn export_json_names_a_broken_file() {
    let tmp = tempfile::tempdir().unwrap();
    let garbage = tricky(tmp.path()).join("坏的 文件.baez");
    std::fs::write(&garbage, b"PK\x03\x04 not really a zip").unwrap();
    for args in [
        vec!["export-json"],
        vec!["render", "--comp", "x", "-o", "o_####.png"],
    ] {
        let out = bae().args(&args).arg(&garbage).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1));
        assert!(
            stderr.contains("坏的 文件.baez"),
            "{args:?} does not name the file: {stderr}"
        );
    }
}

#[test]
fn export_json_matches_the_model_for_bae_and_baez() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    for name in ["导出 测试.bae", "导出 测试.baez"] {
        let path = dir.join(name);
        let want = make_project(&path);
        let out = bae().arg("export-json").arg(&path).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got: Json = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            got, want,
            "{name}: export-json differs from the saved stage"
        );
        // -o into a path with spaces.
        let file = dir.join(format!("{name} 输出.json"));
        let out = bae()
            .arg("export-json")
            .arg(&path)
            .arg("--out")
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let written: Json = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(written, want);
    }
}

#[test]
fn render_png_into_a_tricky_path() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("渲染 工程.bae");
    make_project(&project);
    let comp = bae_motion::query::comps(&Project::open(&project).unwrap().stage)[0].to_string();
    let out_dir = dir.join("输出 帧");
    let out = bae()
        .arg("render")
        .arg(&project)
        .args([
            "--comp", &comp, "--frames", "all", "-f", "png", "--json", "-o",
        ])
        .arg(out_dir.join("帧 _####.png"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let n = std::fs::read_dir(&out_dir).unwrap().count();
    assert_eq!(n, 60, "2 s at 30 fps");
    assert!(out_dir.join("帧 _0000.png").is_file());
}

#[test]
fn smoke_test_with_a_project_and_a_screenshot() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("冒烟 工程.baez");
    make_project(&project);
    let shot = dir.join("截图 shot.bmp");
    let out = bae()
        .args(["--headless", "--smoke-test", "10"])
        .arg(&project)
        .env("BAE_SMOKE_SCREENSHOT", &shot)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() && String::from_utf8_lossy(&out.stderr).contains("adapter") {
        eprintln!("no GPU: skipped");
        return;
    }
    assert!(
        out.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("smoke test passed"), "{stdout}");
    let bmp = std::fs::read(&shot).unwrap();
    assert!(
        bmp.starts_with(b"BM") && bmp.len() > 1280 * 720 * 3,
        "{} bytes",
        bmp.len()
    );
}

/// A JSON-RPC session with `bae mcp`.
struct Mcp {
    child: std::process::Child,
    out: BufReader<std::process::ChildStdout>,
    id: i64,
}

impl Mcp {
    fn start(project: &Path) -> Mcp {
        let mut child = bae()
            .args(["mcp", "--create", "--name", "acceptance", "--audit-log"])
            .arg(project.with_extension("audit.jsonl"))
            .arg(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut mcp = Mcp { child, out, id: 0 };
        let init = mcp.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "acceptance", "version": "1"}}),
        );
        assert!(
            init["result"]["capabilities"]["tools"].is_object(),
            "{init}"
        );
        mcp.notify("notifications/initialized");
        mcp
    }

    fn notify(&mut self, method: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "method": method})).unwrap();
    }

    fn request(&mut self, method: &str, params: Json) -> Json {
        self.id += 1;
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params})
        )
        .unwrap();
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"))
    }

    fn call(&mut self, tool: &str, args: Json) -> Json {
        let reply = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert_eq!(reply["result"]["isError"], false, "{tool}: {reply}");
        reply["result"]["structuredContent"].clone()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_builds_animates_and_saves_a_project() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("智能体 工程.bae");
    {
        let mut mcp = Mcp::start(&project);
        let tools = mcp.request("tools/list", json!({}));
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        for t in [
            "comp_create",
            "layer_create",
            "attr_set_at",
            "stage_describe",
        ] {
            assert!(names.contains(&t), "{t} missing from {names:?}");
        }
        let comp = mcp.call(
            "comp_create",
            json!({"name": "智能 合成", "fps": 30, "duration": 3 * SEC}),
        );
        let comp = comp["output"]["path"].as_str().unwrap().to_owned();
        let layer = mcp.call(
            "layer_create",
            json!({"comp": comp, "kind": "solid", "name": "动画 层"}),
        );
        let layer = layer["output"]["path"].as_str().unwrap().to_owned();
        // Stopwatch on (AE: values set without it change the static value).
        mcp.call(
            "attr_set_animated",
            json!({"path": layer, "attr": "transform:position", "animated": true, "time": 0}),
        );
        for (t, x) in [(0, 100.0), (SEC, 900.0), (2 * SEC, 400.0)] {
            mcp.call(
                "attr_set_at",
                json!({"path": layer, "attr": "transform:position", "time": t,
                       "value": {"type": "vec3", "value": [x, 300.0, 0.0]}}),
            );
        }
        // `bae mcp` saves after every edit (autosave is on by default).
    }
    let out = bae().arg("export-json").arg(&project).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("动画 层") && text.contains("智能 合成"),
        "{text}"
    );
    let stage = Project::open(&project).unwrap().stage;
    let layer = bae_motion::query::comps(&stage)
        .first()
        .map(|c| bae_motion::query::comp_layers(&stage, c)[0].clone())
        .unwrap();
    let keys = &bae_motion::query::attr_spec(&stage, &layer, "transform:position")
        .unwrap()
        .keyframes;
    assert_eq!(keys.len(), 3, "{keys:?}");
}
