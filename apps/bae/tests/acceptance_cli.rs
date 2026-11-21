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
        // Issue #111: the error is printed once, without a log line in front.
        assert_eq!(
            stderr.matches("error:").count(),
            1,
            "{args:?} printed the error more than once: {stderr}"
        );
        assert!(
            !stderr.contains("ERROR") && !stderr.contains("bae_platform::cli"),
            "{args:?} printed a log line: {stderr}"
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

#[test]
fn cli_errors_name_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let garbage = dir.join("坏的 文件.baez");
    std::fs::write(&garbage, b"PK\x03\x04 not really a zip").unwrap();
    let missing = dir.join("没有 这个.bae");
    for (path, args) in [
        (&garbage, vec!["export-json"]),
        (&missing, vec!["export-json"]),
        (&garbage, vec!["--headless"]),
        (&missing, vec!["--headless"]),
    ] {
        let out = bae().args(&args).arg(path).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        println!("{args:?} {}:\n{stderr}", path.display());
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let file = path.file_name().unwrap().to_string_lossy();
        assert!(stderr.contains(&*file), "{args:?}: {stderr}");
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
    }
    // render with a broken project.
    let out = bae()
        .args(["render", "--comp", "x", "-o"])
        .arg(tmp.path().join("o_####.png"))
        .arg(&garbage)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("render:\n{stderr}");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("坏的 文件.baez"), "{stderr}");
}

#[test]
fn cli_errors_are_printed_once_without_a_log_line() {
    // `bae export-json` / `bae render` print the error twice: a timestamped
    // `ERROR bae_platform::cli: …` log line, then `bae …: error: …`.
    let tmp = tempfile::tempdir().unwrap();
    let garbage = tricky(tmp.path()).join("坏的 文件.baez");
    std::fs::write(&garbage, b"PK\x03\x04 not really a zip").unwrap();
    for args in [
        vec!["export-json"],
        vec!["render", "--comp", "x", "-o", "o_####.png"],
    ] {
        let out = Command::new(BAE)
            .args(&args)
            .arg(&garbage)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains(" ERROR "),
            "{args:?}: log line in the error output: {stderr}"
        );
    }
}

/// Issue #131: `bae pack` writes a self-contained .baez (no longer "not
/// implemented"), keeps the project's saved view, and names the file when it
/// cannot.
#[test]
fn pack_writes_a_baez_and_names_the_file_on_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("打包 工程.bae");
    let want = make_project(&project);
    // The desktop app's saved view rides along in project.json.
    let manifest = project.join("project.json");
    let mut m: Json = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    let view = json!({"comps": [{"path": "/comps/x", "time": 705600000}], "active": "/comps/x"});
    m["view"] = view.clone();
    std::fs::write(&manifest, serde_json::to_vec_pretty(&m).unwrap()).unwrap();

    // Default output: next to the project, same name.
    let out = bae().arg("pack").arg(&project).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let baez = dir.join("打包 工程.baez");
    assert!(stdout.contains("打包 工程.baez"), "{stdout}");
    assert!(baez.is_file());
    let exported = bae().arg("export-json").arg(&baez).output().unwrap();
    let got: Json = serde_json::from_slice(&exported.stdout).unwrap();
    assert_eq!(got, want, "the package differs from the project");
    let packed = bae_package::read_manifest(&baez).unwrap();
    assert_eq!(packed.extra.get("view"), Some(&view), "view lost");

    // -o and --json; an existing output is refused without --overwrite.
    let other = dir.join("输出 文件");
    let out = bae()
        .arg("pack")
        .arg(&project)
        .arg("-o")
        .arg(&other)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["kind"], "baez");
    assert!(
        dir.join("输出 文件.baez").is_file(),
        "the .baez extension is added"
    );
    let out = bae().arg("pack").arg(&project).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr.contains("打包 工程.baez") && stderr.contains("--overwrite"),
        "{stderr}"
    );
    assert_eq!(stderr.matches("error:").count(), 1, "{stderr}");
    let out = bae()
        .arg("pack")
        .arg(&project)
        .arg("--overwrite")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // A project that does not open.
    let missing = dir.join("没有 这个.bae");
    let out = bae().arg("pack").arg(&missing).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr.contains("没有 这个.bae") && stderr.contains("does not exist"),
        "{stderr}"
    );
    assert!(!stderr.contains("not implemented"), "{stderr}");
}

impl Mcp {
    /// A tool call; returns (is_error, text).
    fn try_call(&mut self, tool: &str, args: Json) -> (bool, String) {
        let reply = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let is_error = reply["result"]["isError"] == true;
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        (is_error, text)
    }
}

#[test]
fn mcp_agents_cannot_edit_locked_layers() {
    // AE scripts cannot change a locked layer either (docs/spec/ae
    // project-comp-layers.md, Lock); only the Lock switch itself.
    let tmp = tempfile::tempdir().unwrap();
    let project = tricky(tmp.path()).join("锁定 智能体.bae");
    let mut mcp = Mcp::start(&project);
    let comp = mcp.call(
        "comp_create",
        json!({"name": "锁定 合成", "fps": 30, "duration": 2 * SEC}),
    );
    let comp = comp["output"]["path"].as_str().unwrap().to_owned();
    let layer = mcp.call(
        "layer_create",
        json!({"comp": comp, "kind": "solid", "name": "锁定 层"}),
    );
    let layer = layer["output"]["path"].as_str().unwrap().to_owned();
    let other = mcp.call(
        "layer_create",
        json!({"comp": comp, "kind": "null", "name": "其他"}),
    );
    let other = other["output"]["path"].as_str().unwrap().to_owned();
    let (err, text) = mcp.try_call(
        "attr_set",
        json!({"path": layer, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    assert!(!err, "locking: {text}");
    let pos = json!({"type": "vec3", "value": [10.0, 20.0, 0.0]});
    let attempts = [
        (
            "attr_set_at",
            json!({"path": layer, "attr": "transform:position", "time": 0, "value": pos}),
        ),
        (
            "attr_set position",
            json!({"path": layer, "attr": "transform:position", "value": pos}),
        ),
        (
            "attr_set name",
            json!({"path": layer, "attr": "name", "value": {"type": "string", "value": "改了"}}),
        ),
        (
            "attr_set_animated",
            json!({"path": layer, "attr": "transform:opacity", "animated": true, "time": 0}),
        ),
        (
            "layer_set_parent",
            json!({"layers": [layer], "parent": other}),
        ),
        ("layer_delete", json!({"layers": [layer]})),
        ("prim_delete", json!({"path": layer})),
    ];
    let mut allowed = Vec::new();
    for (label, args) in attempts {
        let tool = label.split(' ').next().unwrap();
        let (err, text) = mcp.try_call(tool, args);
        println!(
            "{label}: error={err} {}",
            text.chars().take(160).collect::<String>()
        );
        if !err {
            allowed.push(label);
        }
    }
    // Unlocking still works.
    let (err, text) = mcp.try_call(
        "attr_set",
        json!({"path": layer, "attr": "locked", "value": {"type": "bool", "value": false}}),
    );
    println!(
        "unlock: error={err} {}",
        text.chars().take(120).collect::<String>()
    );
    assert!(
        allowed.is_empty(),
        "an agent edited a locked layer with: {allowed:?}"
    );
}

// ---- Retest of #124 (dev 2c94655): every MCP entry point, batches, history ----

fn pos(x: f64) -> Json {
    json!({"type": "vec3", "value": [x, 20.0, 0.0]})
}

fn lock_args(layer: &str, on: bool) -> Json {
    json!({"path": layer, "attr": "locked", "value": {"type": "bool", "value": on}})
}

/// A comp with three layers; returns (comp, locked, other, third).
fn mcp_three_layers(mcp: &mut Mcp) -> (String, String, String, String) {
    let comp = mcp.call(
        "comp_create",
        json!({"name": "锁 合成", "fps": 30, "duration": 2 * SEC}),
    );
    let comp = comp["output"]["path"].as_str().unwrap().to_owned();
    let mut make = |name: &str, kind: &str| {
        mcp.call(
            "layer_create",
            json!({"comp": comp, "kind": kind, "name": name}),
        )["output"]["path"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let locked = make("锁定 层", "solid");
    let other = make("其他 层", "solid");
    let third = make("第三 层", "null");
    (comp, locked, other, third)
}

#[test]
fn every_mcp_tool_respects_the_lock() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tricky(tmp.path()).join("全部 入口.bae");
    let mut mcp = Mcp::start(&project);
    let (_comp, locked, other, _third) = mcp_three_layers(&mut mcp);
    mcp.call(
        "attr_set_animated",
        json!({"path": other, "attr": "transform:position", "animated": true, "time": 0}),
    );
    mcp.call("history_checkpoint", json!({"name": "锁定前"}));
    let list = mcp.call("history_list", json!({}));
    let checkpoint = list["output"]["checkpoints"][0]["id"]
        .as_str()
        .or_else(|| list["checkpoints"][0]["id"].as_str())
        .unwrap_or_else(|| panic!("no checkpoint id: {list}"))
        .to_owned();
    mcp.call(
        "attr_set",
        json!({"path": locked, "attr": "transform:position", "value": pos(111.0)}),
    );
    let (err, text) = mcp.try_call("attr_set", lock_args(&locked, true));
    assert!(!err, "locking: {text}");
    let refused = [
        (
            "attr_set",
            json!({"path": locked, "attr": "transform:position", "value": pos(1.0)}),
        ),
        (
            "attr_set_many",
            json!({"edits": [{"path": locked, "attr": "transform:opacity", "time": 0, "value": {"type": "float", "value": 50.0}}]}),
        ),
        (
            "attr_clear",
            json!({"path": locked, "attr": "transform:position"}),
        ),
        (
            "attr_set_expression",
            json!({"path": locked, "attr": "transform:rotation", "source": "time * 10"}),
        ),
        (
            "attr_set_at",
            json!({"path": locked, "attr": "transform:position", "time": 0, "value": pos(2.0)}),
        ),
        ("prim_rename", json!({"path": locked, "name": "改名"})),
        (
            "prim_move",
            json!({"from": locked, "to": format!("{locked}_moved")}),
        ),
        (
            "prim_create",
            json!({"path": format!("{locked}/child"), "prim_type": "Group"}),
        ),
        ("prim_delete", json!({"path": locked})),
        ("layer_delete", json!({"layers": [locked]})),
        (
            "layer_shift_time",
            json!({"layers": [locked], "delta": SEC / 2}),
        ),
        (
            "layer_trim",
            json!({"layers": [locked], "in_point": SEC / 2}),
        ),
        (
            "layer_set_parent",
            json!({"layers": [locked], "parent": other}),
        ),
    ];
    let mut wrongly_allowed = Vec::new();
    let mut messages = Vec::new();
    for (tool, args) in refused {
        let (err, text) = mcp.try_call(tool, args);
        messages.push(format!(
            "{tool}: error={err} {}",
            text.chars().take(120).collect::<String>()
        ));
        if !err {
            wrongly_allowed.push(tool);
        } else if !text.contains("锁定 层") {
            wrongly_allowed.push("(message does not name the layer)");
        }
    }
    // Allowed: reading, moving another layer past it, duplicating it.
    let allowed = [
        ("prim_get", json!({"path": locked})),
        ("layer_reorder", json!({"layers": [other], "index": 0})),
        ("layer_duplicate", json!({"layers": [locked]})),
        (
            "attr_set",
            json!({"path": other, "attr": "transform:opacity", "value": {"type": "float", "value": 40.0}}),
        ),
    ];
    let mut wrongly_refused = Vec::new();
    for (tool, args) in allowed {
        let (err, text) = mcp.try_call(tool, args);
        messages.push(format!(
            "{tool}: error={err} {}",
            text.chars().take(120).collect::<String>()
        ));
        if err {
            wrongly_refused.push(tool);
        }
    }
    // Reverting to a version from before the lock restores that version as a
    // whole, lock switch included: the layer comes back unlocked with its old
    // values (like an unlock-edit batch). It must not leave a locked layer
    // with changed values.
    let (err, text) = mcp.try_call("history_revert", json!({"checkpoint": checkpoint}));
    let after = mcp.call("prim_get", json!({"path": locked}));
    let locked_after = after["output"]["attributes"]["locked"]["value"]["value"]
        .as_bool()
        .or_else(|| after["attributes"]["locked"]["value"]["value"].as_bool());
    let after_text = after.to_string();
    messages.push(format!(
        "history_revert: error={err} {} → locked {locked_after:?}, still 111: {}",
        text.chars().take(80).collect::<String>(),
        after_text.contains("111")
    ));
    if !err && locked_after == Some(true) && !after_text.contains("111") {
        wrongly_allowed.push("history_revert (changed a layer that is still locked)");
    }
    // Lock it again for the rest.
    mcp.call("attr_set", lock_args(&locked, true));
    // The agent's own undo replays history (not a new edit).
    let (err, text) = mcp.try_call("history_undo", json!({}));
    messages.push(format!(
        "history_undo: error={err} {}",
        text.chars().take(120).collect::<String>()
    ));
    println!("{}", messages.join("\n"));
    assert!(
        wrongly_allowed.is_empty(),
        "allowed on a locked layer: {wrongly_allowed:?}"
    );
    assert!(
        wrongly_refused.is_empty(),
        "refused although the locked layer is not edited: {wrongly_refused:?}"
    );
    // (history_undo above undid the re-lock.) Lock again: refused again.
    mcp.call("attr_set", lock_args(&locked, true));
    let (err, _) = mcp.try_call(
        "attr_set",
        json!({"path": locked, "attr": "transform:position", "value": pos(3.0)}),
    );
    assert!(err, "not locked at the end");
}

#[test]
fn batches_are_all_or_nothing_and_can_unlock_edit_relock() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tricky(tmp.path()).join("批量.bae");
    let mut mcp = Mcp::start(&project);
    let (_comp, locked, other, _third) = mcp_three_layers(&mut mcp);
    mcp.call("attr_set", lock_args(&locked, true));
    // One step touches the locked layer: nothing of the batch is applied.
    let (err, text) = mcp.try_call(
        "command_batch",
        json!({"commands": [
            {"command": "attr_set", "params": {"path": other, "attr": "transform:position", "value": pos(500.0)}},
            {"command": "attr_set", "params": {"path": locked, "attr": "transform:position", "value": pos(600.0)}},
        ]}),
    );
    assert!(err, "the batch went through: {text}");
    assert!(text.contains("锁定 层"), "message: {text}");
    let other_now = mcp.call("prim_get", json!({"path": other})).to_string();
    assert!(
        !other_now.contains("500"),
        "half of the batch was applied: {other_now}"
    );
    // Unlock, edit, lock again in one batch: allowed (the lock switch itself).
    let (err, text) = mcp.try_call(
        "command_batch",
        json!({"commands": [
            {"command": "attr_set", "params": lock_args(&locked, false)},
            {"command": "attr_set", "params": {"path": locked, "attr": "transform:position", "value": pos(700.0)}},
            {"command": "attr_set", "params": lock_args(&locked, true)},
        ]}),
    );
    assert!(!err, "unlock-edit-relock batch: {text}");
    let now = mcp.call("prim_get", json!({"path": locked})).to_string();
    assert!(now.contains("700"), "{now}");
    // And it is locked again.
    let (err, _) = mcp.try_call(
        "attr_set",
        json!({"path": locked, "attr": "transform:position", "value": pos(800.0)}),
    );
    assert!(err, "not locked after the batch");
    // Undo the batch (one step): unlocked, back at the old value? (history replay)
    let (err, text) = mcp.try_call("history_undo", json!({}));
    assert!(!err, "undo of the batch: {text}");
    let after_undo = mcp.call("prim_get", json!({"path": locked})).to_string();
    assert!(
        !after_undo.contains("700"),
        "undo did not revert the batch: {after_undo}"
    );
    let (err, text) = mcp.try_call("history_redo", json!({}));
    assert!(!err, "redo: {text}");
    assert!(
        mcp.call("prim_get", json!({"path": locked}))
            .to_string()
            .contains("700")
    );
}

#[test]
fn a_lock_from_another_sublayer_binds_the_agent() {
    // The user locks the layer (root sublayer); an agent writing to its own
    // sublayer cannot edit it, but can switch the lock off like any user.
    let tmp = tempfile::tempdir().unwrap();
    let project = tricky(tmp.path()).join("别的 层.bae");
    let layer = {
        let mut builder = bae_app::AppBuilder::new();
        builder.add_module(bae_motion::MotionModule);
        let app = builder.build();
        let mut reg = bae_model::CommandRegistry::with_builtins();
        if let Some(r) = app.get_registry::<dyn bae_model::DynCommand>() {
            for e in r.snapshot().iter() {
                let _ = reg.register_dyn(e.item().clone());
            }
        }
        let mut p = Project::new();
        let author = bae_model::AuthorId::new("user:acceptance").unwrap();
        let mut run = |id: &str, a: Json| reg.execute(&mut p.stage, &author, id, a).unwrap().output;
        let comp = run(
            "comp.create",
            json!({"name": "用户 合成", "fps": 30, "duration": 2 * SEC}),
        );
        let comp = comp["path"].as_str().unwrap().to_owned();
        let l = run(
            "layer.create",
            json!({"comp": comp, "kind": "solid", "name": "用户 锁定"}),
        );
        let l = l["path"].as_str().unwrap().to_owned();
        run("attr.set", lock_args(&l, true));
        p.save_as(&project).unwrap();
        l
    };
    let mut mcp = Mcp::start(&project); // own-layer: writes go to agent/acceptance
    let (err, text) = mcp.try_call(
        "attr_set",
        json!({"path": layer, "attr": "transform:position", "value": pos(5.0)}),
    );
    assert!(err, "the agent edited a layer the user locked: {text}");
    let (err, text) = mcp.try_call("prim_delete", json!({"path": layer}));
    assert!(err, "the agent deleted a layer the user locked: {text}");
    let (err, text) = mcp.try_call("attr_set", lock_args(&layer, false));
    assert!(!err, "the agent cannot switch the lock off: {text}");
    let (err, text) = mcp.try_call(
        "attr_set",
        json!({"path": layer, "attr": "transform:position", "value": pos(6.0)}),
    );
    assert!(!err, "after unlocking: {text}");
}

// ---- Retest of #131 / #111 (dev 277f11a): bae pack, CLI errors ----

/// A `.bae` with two comps, a saved view (second comp active at 1 s) and a
/// field this version does not know.
fn project_with_view(path: &Path) {
    make_project(path);
    let manifest = path.join("project.json");
    let mut j: Json = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    let comp = bae_motion::query::comps(&Project::open(path).unwrap().stage)[0].to_string();
    j["view"] = json!({"comps": [{"path": comp, "time": SEC}], "active": comp});
    j["x_future_field"] = json!({"keep": "me"});
    std::fs::write(&manifest, serde_json::to_vec_pretty(&j).unwrap()).unwrap();
}

/// The top-level fields of a `.baez`'s project.json that are not project data.
fn baez_manifest(path: &Path) -> Json {
    let project = Project::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Json::Object(project.manifest.extra.clone())
}

#[test]
fn bae_pack_keeps_the_view_and_unknown_fields_and_guards_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("打包 测试.bae");
    project_with_view(&project);
    let want_view =
        serde_json::from_slice::<Json>(&std::fs::read(project.join("project.json")).unwrap())
            .unwrap()["view"]
            .clone();
    // Default output: next to the project, same name.
    let out = bae().arg("pack").arg(&project).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let packed = dir.join("打包 测试.baez");
    assert!(packed.is_file(), "no {}", packed.display());
    let m = baez_manifest(&packed);
    assert_eq!(m["view"], want_view, "view lost by bae pack: {m}");
    assert_eq!(
        m["x_future_field"],
        json!({"keep": "me"}),
        "unknown field lost: {m}"
    );
    // Same stage.
    let a = bae_model::export_flattened_json(&Project::open(&project).unwrap().stage);
    let b = bae_model::export_flattened_json(&Project::open(&packed).unwrap().stage);
    assert_eq!(a, b);
    // Again without --overwrite: refused, file untouched; with it: replaced.
    let before = std::fs::metadata(&packed).unwrap().modified().unwrap();
    let out = bae().arg("pack").arg(&project).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && stderr.contains("--overwrite"),
        "{stderr}"
    );
    assert_eq!(
        std::fs::metadata(&packed).unwrap().modified().unwrap(),
        before
    );
    let out = bae()
        .arg("pack")
        .arg(&project)
        .arg("--overwrite")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // -o into another folder with spaces; re-packing a .baez keeps the view too.
    let other = dir.join("另一个 目录");
    std::fs::create_dir(&other).unwrap();
    let repacked = other.join("再 打包.baez");
    let out = bae()
        .arg("pack")
        .arg(&packed)
        .arg("-o")
        .arg(&repacked)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        baez_manifest(&repacked)["view"],
        want_view,
        "view lost when re-packing a .baez"
    );
    // Packing a .baez onto itself is refused.
    let out = bae()
        .arg("pack")
        .arg(&packed)
        .arg("-o")
        .arg(&packed)
        .arg("--overwrite")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("pack onto itself: {stderr}");
    assert!(!out.status.success(), "packing onto itself went through");
    assert!(Project::open(&packed).is_ok(), "the package was damaged");
}

#[test]
fn bae_pack_messages_are_chinese_in_the_chinese_ui() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("中文 提示.bae");
    project_with_view(&project);
    let run = || {
        bae()
            .env("BAE_LANG", "zh")
            .arg("pack")
            .arg(&project)
            .output()
            .unwrap()
    };
    let first = run();
    let stdout = String::from_utf8_lossy(&first.stdout);
    println!("zh pack: {stdout}");
    assert!(first.status.success());
    assert!(stdout.contains("已打包"), "{stdout}");
    let again = run();
    let stderr = String::from_utf8_lossy(&again.stderr);
    println!("zh pack again: {stderr}");
    assert!(
        stderr.contains("已存在；要替换请加 --overwrite"),
        "{stderr}"
    );
}

#[test]
fn mcp_package_pack_keeps_the_view() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let project = dir.join("智能体 打包.bae");
    project_with_view(&project);
    let want =
        serde_json::from_slice::<Json>(&std::fs::read(project.join("project.json")).unwrap())
            .unwrap()["view"]
            .clone();
    let out = dir.join("智能体 打包.baez");
    {
        let mut mcp = Mcp::start(&project);
        mcp.call("package_pack", json!({"output": out}));
    }
    assert_eq!(
        baez_manifest(&out)["view"],
        want,
        "package_pack lost the view"
    );
}

#[test]
fn every_cli_error_is_printed_once_and_names_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let broken = dir.join("坏的 包.baez");
    std::fs::write(&broken, b"PK\x03\x04 nope").unwrap();
    let missing = dir.join("没有 的.bae");
    let runs: Vec<(&str, Vec<std::ffi::OsString>)> = vec![
        (
            "export-json broken",
            vec!["export-json".into(), broken.clone().into()],
        ),
        (
            "export-json missing",
            vec!["export-json".into(), missing.clone().into()],
        ),
        (
            "render broken",
            vec![
                "render".into(),
                "--comp".into(),
                "x".into(),
                "-o".into(),
                tmp.path().join("o_####.png").into(),
                broken.clone().into(),
            ],
        ),
        ("pack broken", vec!["pack".into(), broken.clone().into()]),
        ("pack missing", vec!["pack".into(), missing.clone().into()]),
        (
            "headless broken",
            vec!["--headless".into(), broken.clone().into()],
        ),
        (
            "mcp missing (no --create)",
            vec!["mcp".into(), missing.clone().into()],
        ),
    ];
    let mut problems = Vec::new();
    for (label, args) in runs {
        let out = Command::new(BAE)
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        let file = if label.contains("missing") {
            "没有 的.bae"
        } else {
            "坏的 包.baez"
        };
        let mentions = stderr.matches(file).count();
        println!("{label}: exit {:?}\n{stderr}", out.status.code());
        if out.status.success() {
            problems.push(format!("{label}: exit 0"));
        }
        if stderr.contains(" ERROR ") || stderr.contains("panicked") {
            problems.push(format!("{label}: log line or panic in the output"));
        }
        if mentions == 0 {
            problems.push(format!("{label}: does not name the file"));
        }
        if stderr.lines().filter(|l| l.contains(file)).count() > 2 {
            problems.push(format!("{label}: the error is repeated"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
#[ignore = "issue #111"]
fn mcp_says_a_missing_project_does_not_exist() {
    // `bae mcp <missing>` (without --create) still says "is not a BAE project
    // (expected a .bae directory or a .baez file)" for a path that does not
    // exist; the other entry points say "… does not exist." (#111).
    let tmp = tempfile::tempdir().unwrap();
    let missing = tricky(tmp.path()).join("没有 的.bae");
    let out = Command::new(BAE)
        .arg("mcp")
        .arg(&missing)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(!stderr.contains("is not a BAE project"), "{stderr}");
}

/// Issue #111: every entry point that takes a project path explains a project
/// that does not open in the same words (`file_error::describe_open`), naming
/// the file — never the model's "is not a BAE project" for a missing path.
#[test]
fn every_entry_point_explains_open_failures_the_same_way() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tricky(tmp.path());
    let missing = dir.join("没有 的.bae");
    let garbage = dir.join("坏的 包.baez");
    std::fs::write(&garbage, b"PK\x03\x04 not really a zip").unwrap();
    let out_png = tmp.path().join("o_####.png");
    let entries: Vec<(&str, Vec<std::ffi::OsString>)> = vec![
        ("export-json", vec!["export-json".into()]),
        (
            "render",
            vec![
                "render".into(),
                "--comp".into(),
                "x".into(),
                "-o".into(),
                out_png.clone().into(),
            ],
        ),
        ("pack", vec!["pack".into()]),
        ("mcp", vec!["mcp".into()]),
        ("--headless", vec!["--headless".into()]),
        (
            "--headless --smoke-test",
            vec!["--headless".into(), "--smoke-test".into(), "1".into()],
        ),
        // `bae <project>` opening the window; the smoke test exits before any
        // window when the project does not open.
        ("--smoke-test", vec!["--smoke-test".into(), "1".into()]),
    ];
    for (path, why) in [
        (&missing, "does not exist"),
        (&garbage, "is damaged or is not a .baez project package"),
    ] {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        for (name, args) in &entries {
            let out = Command::new(BAE)
                .args(args)
                .arg(path)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{name} {file}: {stderr}");
            assert!(
                stderr.contains(&file) && stderr.contains(why),
                "{name} {file}: expected “{why}”: {stderr}"
            );
            assert!(
                !stderr.contains("is not a BAE project"),
                "{name} {file}: the model's wording leaked: {stderr}"
            );
        }
    }
}
