//! Plugin host: discovery, version selection, registration, runtime unload.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::{EffectDef, PrimPath, Time};
use bae_plugin::declarative::create_image;
use bae_plugin::host::{EffectRegistries, LoadError, PluginDir, PluginHost, Scope};
use bae_plugin::{DeclarativeEffect, DirSource};
use bae_render::{EffectParams, GpuImage, IRect, RenderEffect, RenderError};

fn manifest(version: &str) -> String {
    format!(
        r#"schema_version = 1
[plugin]
id = "com.example.tint"
version = "{version}"
api = "0.1"
name = "Tint"
[[effect]]
id = "tint"
name = "Tint"
category = "Color Correction"
implementation = "wgsl"
shader = "tint.wgsl"
[[effect.param]]
id = 1
key = "amount"
type = "float"
name = "Amount"
default = 0.5
range = [0.0, 1.0]
"#
    )
}

const SHADER: &str =
    "fn tint(c: vec4<f32>, px: BaePixel) -> vec4<f32> { return c * (1.0 - params.amount); }\n";

fn write_package(root: &Path, version: &str) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("bae-plugin.toml"), manifest(version)).unwrap();
    std::fs::write(root.join("tint.wgsl"), SHADER).unwrap();
    root.to_owned()
}

#[test]
fn discovery_and_version_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let user = tmp.path().join("user");
    let project = tmp.path().join("project");
    // Installed layout <id>/<version>/ and a flat development layout.
    write_package(&user.join("com.example.tint/1.0.0"), "1.0.0");
    write_package(&user.join("com.example.tint/1.2.0"), "1.2.0");
    write_package(&project.join("tint"), "1.2.0");
    std::fs::create_dir_all(user.join(".trash/x")).unwrap();
    std::fs::create_dir_all(user.join("broken")).unwrap();
    std::fs::write(user.join("broken/bae-plugin.toml"), "schema_version = 1\n").unwrap();

    assert_eq!(PluginHost::discover(&user).len(), 3);
    let dirs = [
        PluginDir::new(&user, Scope::User),
        PluginDir::new(&project, Scope::Project),
    ];
    let report = PluginHost::scan(&dirs);
    assert_eq!(report.candidates.len(), 3);
    assert_eq!(report.failed.len(), 1);
    assert!(
        report.failed[0].1[0].message.contains("plugin"),
        "{:?}",
        report.failed
    );
    let active = PluginHost::select(&report.candidates);
    assert_eq!(active.len(), 1);
    // Highest version; the tie between user 1.2.0 and project 1.2.0 goes to the project.
    assert_eq!(active[0].manifest.plugin.version.to_string(), "1.2.0");
    assert_eq!(active[0].scope, Scope::Project);

    let mut regs = EffectRegistries::default();
    let mut host = PluginHost::new();
    let (loaded, problems) = host.load_dirs(&dirs, &mut regs);
    assert_eq!(loaded, ["com.example.tint"]);
    assert_eq!(problems.len(), 1);
    assert_eq!(host.get("com.example.tint").unwrap().scope, Scope::Project);
}

#[test]
fn register_update_and_unload() {
    let tmp = tempfile::tempdir().unwrap();
    let mut regs = EffectRegistries::default();
    let mut host = PluginHost::new();
    let v1 = write_package(&tmp.path().join("v1"), "1.0.0");
    let v2 = write_package(&tmp.path().join("v2"), "2.0.0");

    let p = host
        .load_from(&DirSource::new(&v1), Scope::User, &mut regs)
        .unwrap();
    assert_eq!(p.effects.len(), 1);
    let def = regs.definitions.get("com.example.tint/tint").unwrap();
    assert_eq!(def.plugin.as_ref().unwrap().version, "1.0.0");
    assert_eq!(def.category, "Color Correction");
    assert!(regs.library.get("com.example.tint/tint").is_some());

    // Loading another version replaces the first (update without restart).
    host.load_from(&DirSource::new(&v2), Scope::User, &mut regs)
        .unwrap();
    assert_eq!(host.plugins().len(), 1);
    assert_eq!(
        regs.definitions
            .get("com.example.tint/tint")
            .unwrap()
            .plugin
            .as_ref()
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(regs.library.len(), 1);

    // A render graph holding the effect keeps it alive across the unload.
    let held = regs.library.get("com.example.tint/tint").unwrap().clone();
    let report = host.unload("com.example.tint", &mut regs).unwrap();
    assert_eq!(report.definitions_removed, 1);
    assert_eq!(report.effects_removed, 1);
    assert_eq!(report.effects_in_use, 1);
    assert!(regs.definitions.get("com.example.tint/tint").is_none());
    assert!(regs.library.is_empty());
    assert_eq!(Arc::strong_count(&held), 1);
    drop(held);
    assert!(host.unload("com.example.tint", &mut regs).is_none());
}

#[test]
fn built_in_effects_win_conflicts() {
    let tmp = tempfile::tempdir().unwrap();
    let mut regs = EffectRegistries::default();
    regs.definitions.register(EffectDef {
        match_name: "com.example.tint/tint".into(),
        name: "Built-in".into(),
        category: String::new(),
        plugin: None,
        params: Vec::new(),
        thread_safe: true,
        gpu: true,
    });
    let pkg = write_package(&tmp.path().join("p"), "1.0.0");
    let err = PluginHost::new()
        .load_from(&DirSource::new(&pkg), Scope::User, &mut regs)
        .unwrap_err();
    assert!(matches!(err, LoadError::Conflict { .. }), "{err}");
    assert_eq!(
        regs.definitions.get("com.example.tint/tint").unwrap().name,
        "Built-in"
    );
}

#[test]
fn official_examples_load_through_the_host() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/examples");
    let mut regs = EffectRegistries::default();
    let mut host = PluginHost::new();
    let (loaded, problems) = host.load_dirs(&[PluginDir::new(examples, Scope::Bundled)], &mut regs);
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(
        loaded,
        [
            "com.example.align-to-grid",
            "com.example.box-blur",
            "com.example.posterize",
            "com.example.vignette"
        ]
    );
    assert_eq!(regs.library.len(), 3);
    assert!(
        regs.definitions
            .get("com.example.vignette/vignette")
            .is_some()
    );
    // The script plugin loads (validated) with a warning that it does not run yet.
    assert!(
        !host
            .get("com.example.align-to-grid")
            .unwrap()
            .warnings
            .is_empty()
    );
}

fn context() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

/// Live wgpu objects the application holds (pipelines, layouts, shaders,
/// buffers, textures, views, bind groups).
fn live_objects(gpu: &GpuContext) -> Option<[usize; 8]> {
    let report = gpu.instance().generate_report()?;
    let h = &report.hub;
    Some([
        h.compute_pipelines.num_kept_from_user,
        h.pipeline_layouts.num_kept_from_user,
        h.bind_group_layouts.num_kept_from_user,
        h.shader_modules.num_kept_from_user,
        h.buffers.num_kept_from_user,
        h.textures.num_kept_from_user,
        h.texture_views.num_kept_from_user,
        h.bind_groups.num_kept_from_user,
    ])
}

/// Spec §22.3 acceptance 3: load → render → unload, 100 times, no GPU leak.
#[test]
fn runtime_load_unload_does_not_leak_gpu_resources() {
    let Some(gpu) = context() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let pkg = write_package(&tmp.path().join("p"), "1.0.0");
    let mut regs = EffectRegistries::default();
    let mut host = PluginHost::new();
    let input = create_image(
        &gpu,
        IRect::from_size(32, 32),
        1.0,
        wgpu::TextureFormat::Rgba16Float,
    )
    .unwrap();
    let inst = RenderEffect {
        path: PrimPath::new("/comps/main/layers/a/effects/fx").unwrap(),
        effect: "com.example.tint/tint".into(),
        enabled: true,
        params: Vec::new(),
    };

    let mut cycle = |regs: &mut EffectRegistries| {
        host.load_from(&DirSource::new(&pkg), Scope::User, regs)
            .unwrap();
        let effect = host.get("com.example.tint").unwrap().effects[0].clone();
        let params = EffectParams {
            time: Time::ZERO,
            scale: 1.0,
            ..EffectParams::new(&inst)
        };
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let mut alloc = |r: IRect,
                         s: f64,
                         f: wgpu::TextureFormat|
         -> Result<GpuImage, RenderError> { create_image(&gpu, r, s, f) };
        let out = DeclarativeEffect::render_with(
            &effect,
            &gpu,
            &mut encoder,
            wgpu::TextureFormat::Rgba16Float,
            &mut alloc,
            &params,
            &input,
            input.rect,
        )
        .unwrap();
        gpu.queue().submit([encoder.finish()]);
        drop(out);
        drop(effect);
        let report = host.unload("com.example.tint", regs).unwrap();
        assert_eq!(report.effects_in_use, 0);
        gpu.wait_idle().unwrap();
    };

    // Warm up once (lazily created device-level objects), then measure.
    cycle(&mut regs);
    let Some(baseline) = live_objects(&gpu) else {
        eprintln!("wgpu resource report unavailable; skipping the leak check");
        return;
    };
    for _ in 0..100 {
        cycle(&mut regs);
    }
    assert_eq!(live_objects(&gpu).unwrap(), baseline);
    assert!(regs.definitions.iter().next().is_none() && regs.library.is_empty());
}
