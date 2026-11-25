//! Native plugins end to end, with the host's own loading code: the plain-C
//! example in process, the Rust example out of process, crash isolation,
//! panics in the Rust SDK, the 0.1 pre-release ABI, manifest checks and the
//! GPU → CPU → GPU render node.
//!
//! The C plugins are compiled with the platform C compiler (found by the `cc`
//! crate; tests that need one print a note and pass when there is none unless
//! `BAE_REQUIRE_CC=1`); the Rust example is built with a nested `cargo build`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use bae_model::{PrimPath, Value};
use bae_plugin::native::library::Library;
use bae_plugin::native::{
    CallError, FrameRequest, NativeConfig, NoticeKind, ParamWire, RenderOut, RunnerCommand, pixels,
};
use bae_plugin::{DirSource, EffectRegistries, PluginHost, PluginManifest, Scope};
use bae_plugin_sdk::ApiVersion;
use bae_plugin_sdk::abi::*;
use bae_render::{Effect, EffectParams, IRect, RenderEffect, testing};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn platform() -> &'static str {
    bae_plugin::id::current_platform().expect("supported platform")
}

/// A fresh scratch directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("native-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn triple() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

/// Compiles a C plugin into the shared library `out`; `None` when there is no
/// C compiler (and `BAE_REQUIRE_CC` is not set).
fn build_c(src: &Path, out: &Path, defines: &[&str]) -> Option<()> {
    let tool = cc::Build::new()
        .target(triple())
        .host(triple())
        .opt_level(0)
        .cargo_metadata(false)
        .warnings(false)
        .try_get_compiler();
    let tool = match tool {
        Ok(t) => t,
        Err(e) => {
            assert!(
                std::env::var_os("BAE_REQUIRE_CC").is_none(),
                "no C compiler: {e}"
            );
            eprintln!("note: no C compiler ({e}); skipping");
            return None;
        }
    };
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let sdk = repo().join("sdk").join("c");
    let mut cmd = tool.to_command();
    if tool.is_like_msvc() {
        // C11 for the examples' designated initializers (the header itself
        // also builds in MSVC's default pre-C11 mode).
        cmd.arg("/nologo")
            .arg("/std:c11")
            .arg("/LD")
            .arg(format!("/I{}", sdk.display()));
        for d in defines {
            cmd.arg(format!("/D{d}"));
        }
        cmd.arg(src)
            .arg(format!("/Fe{}", out.display()))
            .arg(format!("/Fo{}\\", out.parent().unwrap().display()));
    } else {
        cmd.args(["-std=c11", "-shared", "-fPIC", "-fvisibility=hidden"])
            .arg(format!("-I{}", sdk.display()));
        for d in defines {
            cmd.arg(format!("-D{d}"));
        }
        cmd.arg(src).arg("-o").arg(out);
    }
    let output = cmd.output().expect("run the C compiler");
    assert!(
        output.status.success(),
        "C compiler failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Some(())
}

/// Copies a package's manifest (and locales) into `dir` and returns the
/// library path the manifest names for this platform.
fn stage_package(src: &Path, dir: &Path, edit: impl Fn(String) -> String) -> PathBuf {
    let manifest = edit(std::fs::read_to_string(src.join("bae-plugin.toml")).unwrap());
    std::fs::write(dir.join("bae-plugin.toml"), &manifest).unwrap();
    if let Ok(entries) = std::fs::read_dir(src.join("locales")) {
        std::fs::create_dir_all(dir.join("locales")).unwrap();
        for e in entries.flatten() {
            std::fs::copy(e.path(), dir.join("locales").join(e.file_name())).unwrap();
        }
    }
    let parsed = PluginManifest::parse(&manifest).unwrap();
    let rel = &parsed.native.as_ref().unwrap().libraries[platform()];
    rel.split('/').fold(dir.to_owned(), |p, s| p.join(s))
}

/// The C invert example as a package in `dir` (`None` without a C compiler).
fn invert_package(
    dir: &Path,
    defines: &[&str],
    edit: impl Fn(String) -> String,
) -> Option<PathBuf> {
    let src = repo().join("sdk/c/examples/invert");
    let lib = stage_package(&src, dir, edit);
    build_c(&src.join("invert.c"), &lib, defines)?;
    Some(lib)
}

/// The crash fixture as a package in `dir`.
fn crash_package(dir: &Path) -> Option<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/crash");
    let lib = stage_package(&src, dir, |m| m);
    build_c(&src.join("crash.c"), &lib, &[])?;
    Some(lib)
}

/// Builds the Rust example once per test run; returns the library.
fn duotone_library() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let example = repo().join("plugins/examples/duotone-rs");
            let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("duotone-target");
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let out = Command::new(cargo)
                .args(["build", "--quiet", "--manifest-path"])
                .arg(example.join("Cargo.toml"))
                .env("CARGO_TARGET_DIR", &target)
                .current_dir(&example)
                .output()
                .expect("run cargo");
            assert!(
                out.status.success(),
                "building duotone-rs failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let name = if cfg!(windows) {
                "duotone_rs.dll"
            } else if cfg!(target_os = "macos") {
                "libduotone_rs.dylib"
            } else {
                "libduotone_rs.so"
            };
            target.join("debug").join(name)
        })
        .clone()
}

fn duotone_package(dir: &Path) -> PathBuf {
    let src = repo().join("plugins/examples/duotone-rs");
    let lib = stage_package(&src, dir, |m| m);
    std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
    std::fs::copy(duotone_library(), &lib).unwrap();
    lib
}

fn runner() -> RunnerCommand {
    RunnerCommand::program(env!("CARGO_BIN_EXE_bae-plugin-runner"))
}

fn config(in_process: bool) -> NativeConfig {
    NativeConfig {
        runner: Some(runner()),
        in_process: if in_process {
            vec!["*".into()]
        } else {
            Vec::new()
        },
        headless: true,
    }
}

const F32: BaePixelFormat = BAE_PIXEL_FORMAT_RGBA_F32_PREMUL;

fn bae_rect(r: [i32; 4]) -> BaeRect {
    BaeRect::new(r[0], r[1], r[2], r[3])
}

/// A CPU image (F32, host row strides) covering `rect`.
fn image(rect: [i32; 4], f: impl Fn(i32, i32) -> [f32; 4]) -> Vec<u8> {
    let r = bae_rect(rect);
    let mut out = vec![0u8; pixels::image_len(F32, r)];
    let stride = pixels::row_bytes(F32, r.width());
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let at = (y - r.y0) as usize * stride + (x - r.x0) as usize * 16;
            for (k, v) in f(x, y).iter().enumerate() {
                out[at + 4 * k..at + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
    out
}

fn pixel(buf: &[u8], rect: [i32; 4], x: i32, y: i32) -> [f32; 4] {
    let r = bae_rect(rect);
    let at = (y - r.y0) as usize * pixels::row_bytes(F32, r.width()) + (x - r.x0) as usize * 16;
    std::array::from_fn(|k| f32::from_le_bytes(buf[at + 4 * k..at + 4 * k + 4].try_into().unwrap()))
}

fn frame(effect: u32, params: Vec<ParamWire>, rect: [i32; 4]) -> FrameRequest {
    FrameRequest {
        effect,
        format: F32,
        time: 0,
        time_step: BAE_FLICKS_PER_SECOND / 30,
        frame_rate: [30, 1],
        downsample: [1, 1],
        quality: BAE_QUALITY_HIGH,
        flags: BAE_RENDER_FLAG_FINAL,
        color_space: BAE_COLOR_SPACE_SRGB,
        shutter: [180.0, -90.0],
        layer_bounds: rect,
        render_id: 1,
        layer_duration: 0,
        params,
        request: rect,
        input: rect,
    }
}

fn source(x: i32, y: i32) -> [f32; 4] {
    let a = 0.5 + 0.125 * ((x + y) % 4) as f32;
    [0.1 * a, 0.4 * a, (x as f32 / 16.0) * a, a]
}

fn close(a: [f32; 4], b: [f32; 4], tol: f32) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
}

/// Native plugin processes are slow to start under parallel test load; run the
/// tests that spawn runners one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn c_example_loads_and_renders_in_process() {
    let dir = scratch("invert");
    let Some(lib) = invert_package(&dir, &[], |m| m) else {
        return;
    };
    let library = Library::load(
        &lib,
        "com.example.invert-c",
        &dir,
        ApiVersion::CURRENT.pack(),
        0,
    )
    .expect("load");
    let info = library.info();
    assert_eq!(info.plugin_id, "com.example.invert-c");
    assert_eq!(info.plugin_version, "0.1.0");
    assert_eq!(info.abi_required, ApiVersion::new(1, 0).pack());
    assert_eq!(info.effects.len(), 1);
    let fx = &info.effects[0];
    assert_eq!(
        (fx.id.as_str(), fx.params[0].key.as_str()),
        ("invert", "amount")
    );
    assert!(!fx.has_pre_render);

    let rect = [10, 20, 18, 24];
    let input = image(rect, source);
    let mut output = vec![0u8; input.len()];
    let out = library
        .render(
            &frame(0, vec![ParamWire::Float(50.0)], rect),
            &input,
            &mut output,
        )
        .expect("render");
    assert_eq!(out, RenderOut::Rendered(rect));
    for y in 20..24 {
        for x in 10..18 {
            let s = source(x, y);
            let want = [
                s[0] + 0.5 * (s[3] - 2.0 * s[0]),
                s[1] + 0.5 * (s[3] - 2.0 * s[1]),
                s[2] + 0.5 * (s[3] - 2.0 * s[2]),
                s[3],
            ];
            assert!(close(pixel(&output, rect, x, y), want, 1e-6), "({x}, {y})");
        }
    }

    // Through the host: registered like any effect, unloaded without a trace.
    let mut host = PluginHost::with_config(config(true));
    let mut reg = EffectRegistries::default();
    let loaded = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .expect("host load");
    let native = loaded.native.as_ref().expect("native plugin");
    assert!(!native.is_out_of_process(), "trusted for in-process use");
    assert_eq!(loaded.native_effects.len(), 1);
    let name = "com.example.invert-c/invert";
    assert!(reg.definitions.get(name).is_some());
    assert!(reg.library.get(name).is_some());
    let report = host.unload("com.example.invert-c", &mut reg).unwrap();
    assert_eq!((report.definitions_removed, report.effects_removed), (1, 1));
    assert!(reg.library.get(name).is_none());
}

#[test]
fn user_plugins_run_out_of_process_unless_trusted() {
    let c = NativeConfig::default();
    use bae_plugin::manifest::Isolation;
    assert!(c.out_of_process("a.b", Scope::User, Isolation::InProcess));
    assert!(c.out_of_process("a.b", Scope::Project, Isolation::InProcess));
    assert!(!c.out_of_process("a.b", Scope::Bundled, Isolation::InProcess));
    assert!(!c.out_of_process("a.b", Scope::System, Isolation::InProcess));
    assert!(c.out_of_process("a.b", Scope::Bundled, Isolation::OutOfProcess));
    assert!(c.out_of_process("a.b", Scope::System, Isolation::PreferOutOfProcess));
    let trusted = NativeConfig {
        in_process: vec!["a.b".into()],
        ..NativeConfig::default()
    };
    assert!(!trusted.out_of_process("a.b", Scope::User, Isolation::InProcess));
    assert!(!trusted.out_of_process("a.b", Scope::User, Isolation::PreferOutOfProcess));
    assert!(trusted.out_of_process("a.b", Scope::User, Isolation::OutOfProcess));
    assert!(trusted.out_of_process("c.d", Scope::User, Isolation::InProcess));
}

#[test]
fn rust_example_renders_out_of_process() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("duotone");
    duotone_package(&dir);
    let mut host = PluginHost::with_config(config(false));
    let mut reg = EffectRegistries::default();
    let loaded = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .expect("host load");
    let native = loaded.native.clone().expect("native plugin");
    assert!(native.is_out_of_process());
    let def = reg
        .definitions
        .get("com.example.duotone-rs/duotone")
        .expect("definition from define_params");
    let keys: Vec<&str> = def.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(keys, ["Shadows", "Highlights", "Amount"]);
    let info = &native.info().effects[0];
    assert_eq!(info.threading, BAE_THREADING_CONCURRENT);

    let rect = [0, 0, 6, 3];
    let input = image(rect, source);
    let mut output = vec![0u8; input.len()];
    let dark = [0.0f32, 0.0, 1.0, 1.0];
    let light = [1.0f32, 0.0, 0.0, 1.0];
    let params = vec![
        ParamWire::Color(dark),
        ParamWire::Color(light),
        ParamWire::Float(100.0),
    ];
    let runner = native.runner().unwrap();
    let out = runner
        .render(&frame(0, params, rect), &input, &mut output)
        .expect("render out of process");
    assert_eq!(out, RenderOut::Rendered(rect));
    for y in 0..3 {
        for x in 0..6 {
            let [r, g, b, a] = source(x, y);
            let l = ((0.2126 * r + 0.7152 * g + 0.0722 * b) / a).clamp(0.0, 1.0);
            let want = [
                (dark[0] + (light[0] - dark[0]) * l) * a,
                0.0,
                (dark[2] + (light[2] - dark[2]) * l) * a,
                a,
            ];
            assert!(close(pixel(&output, rect, x, y), want, 1e-5), "({x}, {y})");
        }
    }
    let pid = runner.pid().expect("runner running");
    assert_ne!(pid, std::process::id());
    host.unload("com.example.duotone-rs", &mut reg).unwrap();
    drop(native);
}

#[test]
fn crashes_and_hard_errors_never_reach_the_host() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("crash");
    if crash_package(&dir).is_none() {
        return;
    }
    let mut host = PluginHost::with_config(config(false));
    let mut reg = EffectRegistries::default();
    let loaded = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .expect("host load");
    let native = loaded.native.clone().unwrap();
    let runner = native.runner().expect("out of process");
    let rect = [0, 0, 4, 4];
    let input = image(rect, source);
    let mut output = vec![0u8; input.len()];
    let mode = |m: i32| frame(0, vec![ParamWire::Int(m)], rect);

    runner.render(&mode(0), &input, &mut output).expect("copy");
    assert!(close(pixel(&output, rect, 2, 1), source(2, 1), 0.0));
    let first = runner.pid().unwrap();

    // An error status is reported with the plugin's message; the runner lives on.
    let e = runner.render(&mode(2), &input, &mut output).unwrap_err();
    assert!(
        matches!(&e, CallError::Status { message, .. } if message.contains("mode 2 always fails")),
        "{e:?}"
    );
    assert_eq!(runner.pid(), Some(first));

    // A segfault kills only the runner; the next call starts a new one.
    let e = runner.render(&mode(1), &input, &mut output).unwrap_err();
    assert!(
        matches!(
            e,
            CallError::Crashed {
                disabled: false,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(runner.pid(), None);
    runner
        .render(&mode(0), &input, &mut output)
        .expect("restarted");
    assert_ne!(runner.pid(), Some(first));

    // abort() too; the third crash within a minute disables the plugin.
    let e = runner.render(&mode(3), &input, &mut output).unwrap_err();
    assert!(
        matches!(
            e,
            CallError::Crashed {
                disabled: false,
                ..
            }
        ),
        "{e:?}"
    );
    let e = runner.render(&mode(1), &input, &mut output).unwrap_err();
    assert!(
        matches!(e, CallError::Crashed { disabled: true, .. }),
        "{e:?}"
    );
    let e = runner.render(&mode(0), &input, &mut output).unwrap_err();
    assert_eq!(e, CallError::Disabled);
    host.unload("com.example.crash", &mut reg);
}

#[test]
fn a_crashing_effect_passes_its_input_through_on_the_gpu() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        eprintln!("note: no GPU; skipping");
        return;
    };
    let dir = scratch("crash-gpu");
    if crash_package(&dir).is_none() {
        return;
    }
    let mut host = PluginHost::with_config(config(false));
    let mut reg = EffectRegistries::default();
    let loaded = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .expect("host load");
    let effect = loaded.native_effects[0].clone();
    let rect = IRect::new(0, 0, 4, 4);
    let pixels_in: Vec<[f32; 4]> = (0..16).map(|i| source(i % 4, i / 4)).collect();
    let input = testing::upload(&gpu, rect, 1.0, &pixels_in).unwrap();
    let instance = RenderEffect {
        path: PrimPath::new("/comps/main/layers/a/effects/fx").unwrap(),
        effect: effect.def().match_name.clone(),
        enabled: true,
        params: vec![(1, Value::Int(1))],
    };
    let params = EffectParams::new(&instance);
    let out = testing::render_effect(&gpu, effect.as_ref(), &params, &input, rect)
        .expect("a plugin crash never fails the frame");
    assert_eq!(out.rect, input.rect);
    let got = testing::read_pixels(&gpu, &out).unwrap();
    for (g, s) in got.iter().zip(&pixels_in) {
        assert!(close(*g, *s, 2e-3), "passed through: {g:?} vs {s:?}");
    }
    let notices = host.notices().take();
    assert!(
        notices
            .iter()
            .any(|n| n.kind == NoticeKind::Crashed && n.plugin == "com.example.crash"),
        "{notices:?}"
    );
}

#[test]
fn the_cpu_node_matches_the_reference_on_the_gpu() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        eprintln!("note: no GPU; skipping");
        return;
    };
    let dir = scratch("invert-gpu");
    if invert_package(&dir, &[], |m| m).is_none() {
        return;
    }
    let mut host = PluginHost::with_config(config(true));
    let mut reg = EffectRegistries::default();
    let loaded = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .expect("host load");
    let effect = loaded.native_effects[0].clone();
    let rect = IRect::new(-3, 5, 5, 9);
    let pixels_in: Vec<[f32; 4]> = (0..32).map(|i| source(i % 8 - 3, i / 8 + 5)).collect();
    let input = testing::upload(&gpu, rect, 1.0, &pixels_in).unwrap();
    let instance = RenderEffect {
        path: PrimPath::new("/comps/main/layers/a/effects/fx").unwrap(),
        effect: effect.def().match_name.clone(),
        enabled: true,
        params: vec![(1, Value::Float(100.0))],
    };
    let params = EffectParams::new(&instance);
    let out = testing::render_effect(&gpu, effect.as_ref(), &params, &input, rect).unwrap();
    assert_eq!(out.rect, rect);
    let got = testing::read_pixels(&gpu, &out).unwrap();
    for (i, (g, s)) in got.iter().zip(&pixels_in).enumerate() {
        let want = [s[3] - s[0], s[3] - s[1], s[3] - s[2], s[3]];
        assert!(close(*g, want, 2e-3), "texel {i}: {g:?} vs {want:?}");
    }
    assert!(host.notices().take().is_empty());
}

#[test]
fn a_0_1_pre_release_plugin_runs_on_the_1_0_host() {
    let dir = scratch("invert-0.1");
    let lib = invert_package(&dir, &["INVERT_ABI_REQUIRED=BAE_MAKE_VERSION(0,1)"], |m| {
        m.replace("api = \"1.0\"", "api = \"0.1\"")
    });
    if lib.is_none() {
        return;
    }
    let mut host = PluginHost::with_config(config(true));
    let mut reg = EffectRegistries::default();
    let loaded = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .expect("0.1 plugins are served as the layout-identical pre-release");
    let info = loaded.native.as_ref().unwrap().info();
    assert_eq!(ApiVersion::unpack(info.abi_required), ApiVersion::new(0, 1));
}

#[test]
fn the_listing_copy_must_match_define_params() {
    let dir = scratch("invert-mismatch");
    let lib = invert_package(&dir, &[], |m| {
        m.replace("key = \"amount\"", "key = \"strength\"")
    });
    if lib.is_none() {
        return;
    }
    let mut host = PluginHost::with_config(config(true));
    let mut reg = EffectRegistries::default();
    let error = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .unwrap_err();
    assert!(error.to_string().contains("define_params"), "{error}");
    assert!(reg.definitions.get("com.example.invert-c/invert").is_none());
    let notices = host.notices().take();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::LoadFailed);
}

#[test]
fn a_missing_library_fails_the_load_with_a_clear_message() {
    let dir = scratch("invert-unbuilt");
    stage_package(&repo().join("sdk/c/examples/invert"), &dir, |m| m);
    let mut host = PluginHost::with_config(config(true));
    let mut reg = EffectRegistries::default();
    let error = host
        .load_from(&DirSource::new(&dir), Scope::User, &mut reg)
        .unwrap_err();
    assert!(error.to_string().contains("has not been built"), "{error}");
}

// ---- A Rust plugin linked into this test binary (SDK panic handling) ----

mod panicking {
    use bae_plugin_sdk::effect::{CpuEffect, Error, Param, RenderArgs};

    #[derive(Default)]
    pub struct Panics;

    impl CpuEffect for Panics {
        const ID: &'static core::ffi::CStr = c"panics";
        const PARAMS: &'static [Param] = &[Param::checkbox(1, c"explode", c"Explode", true)];
        fn render(&self, args: &mut RenderArgs<'_>) -> Result<(), Error> {
            if args.params.checkbox(1).unwrap_or(false) {
                panic!("boom");
            }
            args.map_pixels(|_, _, px| px)
        }
    }

    bae_plugin_sdk::export_plugin!(id: "com.example.panics", version: "1.2.3", effects: [Panics]);
}

#[test]
fn the_rust_sdk_turns_panics_into_errors() {
    let dir = scratch("panics");
    let library = Library::load_static(
        panicking::bae_plugin_entry,
        "com.example.panics",
        &dir,
        ApiVersion::CURRENT.pack(),
        0,
    )
    .expect("load");
    assert_eq!(library.info().plugin_version, "1.2.3");
    assert_eq!(library.info().effects[0].params[0].key, "explode");
    let rect = [0, 0, 2, 2];
    let input = image(rect, source);
    let mut output = vec![0u8; input.len()];
    let e = library
        .render(
            &frame(0, vec![ParamWire::Bool(true)], rect),
            &input,
            &mut output,
        )
        .unwrap_err();
    assert!(
        matches!(&e, CallError::Status { status, message } if *status == BAE_ERR_PANIC && message.contains("boom")),
        "{e:?}"
    );
    library
        .render(
            &frame(0, vec![ParamWire::Bool(false)], rect),
            &input,
            &mut output,
        )
        .expect("the plugin itself survives");
    assert!(close(pixel(&output, rect, 1, 1), source(1, 1), 0.0));
}

#[test]
fn native_and_script_entries_share_the_running_app_and_replace_together() {
    use bae_app::AppBuilder;
    use bae_model::{AuthorId, DynCommand, EffectDef, Stage};
    use bae_plugin::{PluginModule, PluginRuntime};
    use serde_json::json;
    use std::sync::Arc;

    let dir = scratch("mixed-native-script");
    let manifest = r#"
[script]
entry = "main.js"
[[command]]
id = "probe"
name = "Probe"
implementation = "script"
entry = "run"
schema = "args.json"
[[automation]]
id = "report"
name = "Report"
entry = "report"
"#;
    invert_package(&dir, &[], |m| m + manifest).expect("C compiler for mixed package");
    std::fs::write(dir.join("args.json"), "{}").unwrap();
    let js =
        "export function run(args) { return args.value; } export function report() { return 2; }";
    std::fs::write(dir.join("main.js"), js).unwrap();
    let mut app = AppBuilder::new();
    app.add_module(PluginModule);
    let app = app.build();
    let runtime = app.resource::<std::sync::Arc<PluginRuntime>>().unwrap();
    let source = DirSource::new(&dir);
    let id = runtime.load(&source, Scope::Bundled).unwrap();
    let defs = app.get_registry::<EffectDef>().unwrap();
    let effects = app.get_registry::<dyn Effect>().unwrap();
    let commands = app.get_registry::<dyn DynCommand>().unwrap();
    let effect = "com.example.invert-c/invert";
    let command = "plugin.com.example.invert-c.probe";
    let automation = "script.com.example.invert-c.report";
    let author = AuthorId::new("user:artist").unwrap();
    let mut stage = Stage::new();
    let mut invoke = |id: &str| {
        stage
            .transact(&author, id, |tx| {
                commands.get(id).unwrap().run_json(tx, json!({"value": 17}))
            })
            .unwrap()
            .0
    };
    assert!(defs.get(effect).is_some());
    let native = effects.get(effect).unwrap();
    assert_eq!(invoke(command), json!(17));
    assert_eq!(invoke(automation), json!(2));
    std::fs::write(dir.join("main.js"), "export function run() {}").unwrap();
    assert!(
        runtime
            .load(&source, Scope::Bundled)
            .unwrap_err()
            .to_string()
            .contains("report")
    );
    assert!(Arc::ptr_eq(&native, &effects.get(effect).unwrap()));
    assert_eq!(invoke(command), json!(17));
    assert_eq!(invoke(automation), json!(2));
    std::fs::write(dir.join("main.js"), js.replace("return 2", "return 3")).unwrap();
    runtime.load(&source, Scope::Bundled).unwrap();
    assert!(effects.get(effect).is_some());
    assert_eq!(invoke(command), json!(17));
    assert_eq!(invoke(automation), json!(3));
    runtime.unload(&id).unwrap();
    assert!(effects.get(effect).is_none() && defs.get(effect).is_none());
    assert!(commands.get(command).is_none() && commands.get(automation).is_none());
}
