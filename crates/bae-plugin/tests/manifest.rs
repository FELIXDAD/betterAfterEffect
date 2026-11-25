//! Manifest parsing and validation, and the official example packages.

use std::path::PathBuf;

use bae_model::{ParamKind, Value};
use bae_plugin::diag::has_errors;
use bae_plugin::manifest::{EffectFlag, ShaderStyle};
use bae_plugin::wgsl::{self, WorkingFormat};
use bae_plugin::{DirSource, MemorySource, Package, PluginManifest};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(dir: &str) -> Package {
    match Package::load(&DirSource::new(repo().join(dir))) {
        Ok(p) => p,
        Err(diags) => {
            let text: Vec<String> = diags.iter().map(ToString::to_string).collect();
            panic!("{dir} failed to load:\n{}", text.join("\n"));
        }
    }
}

#[test]
fn vignette_example() {
    let p = load("plugins/examples/vignette");
    assert_eq!(p.id(), "com.example.vignette");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    let e = &p.effects[0];
    assert_eq!(e.def.match_name, "com.example.vignette/vignette");
    assert_eq!(e.def.plugin.as_ref().unwrap().version, "1.0.0");
    assert!(e.def.gpu && e.def.thread_safe);
    assert_eq!(e.decl.shader_style(), ShaderStyle::Pixel);
    assert_ne!(e.decl.flag_bits() & EffectFlag::PixelIndependent.bit(), 0);
    let ids: Vec<u32> = e.def.params.iter().map(|p| p.id).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5, 6]);
    let amount = e.def.param(1).unwrap();
    assert!(matches!(
        amount.kind,
        ParamKind::Slider { valid_min: 0.0, valid_max: 100.0, integer: false, ref display, .. } if display == "percent"
    ));
    assert_eq!(amount.default, Some(Value::Float(50.0)));
    // Relative point: the default is a fraction of the layer size.
    let center = e.def.param(2).unwrap();
    assert_eq!(center.kind, ParamKind::Point);
    assert!(center.flags.relative_default);
    assert_eq!(center.default, Some(Value::Vec2([0.5, 0.5])));
    let shape = e.def.param(5).unwrap();
    assert_eq!(
        shape.kind,
        ParamKind::Popup {
            options: vec!["Circle".into(), "Fit Layer".into()]
        }
    );
    assert_eq!(shape.default, Some(Value::Int(1)));
    assert_eq!(
        e.def.param(6).unwrap().default,
        Some(Value::Color([0.0, 0.0, 0.0, 1.0]))
    );
    // The definition round-trips through the model's JSON form (agents, UI).
    let json = serde_json::to_value(&e.def).unwrap();
    let back: bae_model::EffectDef = serde_json::from_value(json).unwrap();
    assert_eq!(back, e.def);
}

#[test]
fn box_blur_example() {
    let p = load("plugins/examples/box-blur");
    let e = &p.effects[0];
    assert_eq!(e.decl.shader_style(), ShaderStyle::Compute);
    assert_ne!(e.decl.flag_bits() & EffectFlag::ExpandsBuffer.bit(), 0);
    let modules =
        wgsl::compile_effect(&e.decl, &e.shader_files, WorkingFormat::Rgba32Float).unwrap();
    assert_eq!(modules.len(), 2);
    assert!(modules[1].assembled.source.contains("rgba32float"));
}

#[test]
fn script_and_c_examples() {
    let p = load("plugins/examples/align-to-grid");
    assert_eq!(p.manifest.commands[0].id, "align-to-grid");
    assert_eq!(p.manifest.automations[0].id, "report-off-grid");
    let p = load("sdk/c/examples/invert");
    let e = &p.effects[0];
    assert_eq!(e.def.match_name, "com.example.invert-c/invert");
    assert!(!e.def.gpu && e.def.thread_safe);
    // Libraries are build outputs: warnings, not errors.
    assert!(
        p.warnings
            .iter()
            .all(|d| d.message.contains("has not been built"))
    );
}

const HEADER: &str = r#"schema_version = 1
[plugin]
id = "com.example.t"
version = "1.0.0"
api = "0.1"
name = "T"
"#;

fn effect_with(body: &str) -> String {
    format!(
        "{HEADER}\n[[effect]]\nid = \"fx\"\nname = \"Fx\"\nimplementation = \"wgsl\"\nshader = \"fx.wgsl\"\n{body}"
    )
}

/// Validation errors (not parse errors) of a manifest.
fn errors(toml: &str) -> Vec<String> {
    let m = PluginManifest::parse(toml).unwrap_or_else(|e| panic!("parse: {e}"));
    m.validate()
        .into_iter()
        .filter(|d| d.is_error())
        .map(|d| d.to_string())
        .collect()
}

fn assert_error(toml: &str, needle: &str) {
    let errs = errors(toml);
    assert!(
        errs.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}, got {errs:#?}"
    );
}

#[test]
fn parse_errors_have_locations() {
    let bad = format!("{HEADER}colour = 1\n");
    let e = PluginManifest::parse(&bad).unwrap_err().0;
    assert_eq!(e.line, Some(7));
    assert!(e.message.contains("colour"), "{e}");

    let bad = HEADER.replace("1.0.0", "1.0");
    let e = PluginManifest::parse(&bad).unwrap_err().0;
    assert!(e.message.contains("invalid version"), "{e}");
    assert_eq!(e.line, Some(4));

    let bad =
        effect_with("[[effect.param]]\nid = 1\nkey = \"a\"\ntype = \"slider\"\nname = \"A\"\n");
    let e = PluginManifest::parse(&bad).unwrap_err().0;
    assert!(e.message.contains("slider"), "{e}");
}

#[test]
fn validation_errors() {
    assert_error(&HEADER.replace("com.example.t", "Example"), "plugin id");
    assert_error(
        &HEADER.replace("\"0.1\"", "\"1.0\""),
        "not supported by this host",
    );
    assert_error(
        &effect_with("").replace("\"fx\"", "\"Fx\""),
        "must start with a lowercase",
    );
    assert_error(
        &effect_with("").replace("fx.wgsl", "../fx.wgsl"),
        "relative to the package root",
    );
    let param = |extra: &str| {
        effect_with(&format!(
            "[[effect.param]]\nid = 1\nkey = \"a\"\ntype = \"float\"\nname = \"A\"\n{extra}"
        ))
    };
    assert_error(
        &param("default = 5.0\nrange = [0.0, 1.0]\n"),
        "outside range",
    );
    assert_error(
        &param("slider = [0.0, 10.0]\nrange = [0.0, 1.0]\n"),
        "slider must lie within range",
    );
    assert_error(
        &param("choices = [\"x\"]\n"),
        "`choices` is not valid for float params",
    );
    assert_error(
        &(param("") + "[[effect.param]]\nid = 1\nkey = \"b\"\ntype = \"checkbox\"\nname = \"B\"\n"),
        "duplicate param id 1",
    );
    assert_error(
        &effect_with(
            "[[effect.param]]\nid = 1\nkey = \"m\"\ntype = \"popup\"\nname = \"M\"\nchoices = [\"a\"]\ndefault = 2\n",
        ),
        "1-based choice index",
    );
    assert_error(
        &effect_with("[[effect.param]]\nid = 1\nkey = \"s\"\ntype = \"string\"\nname = \"S\"\n"),
        "reserved",
    );
    assert_error(
        &effect_with("[[effect.param]]\nid = 1\nkey = \"g\"\ntype = \"group\"\nname = \"G\"\n"),
        "end_id",
    );
    assert_error(
        &effect_with("identity_when = \"params.nope == 0\"\n"),
        "unknown param `nope`",
    );
    assert_error(
        &effect_with("[effect.roi]\nread = \"4\"\n"),
        "pixel-style effects read only their own pixel",
    );
    assert_error(
        &effect_with("[[effect.pass]]\nentry = \"a\"\nreads = [\"tmp\"]\nwrites = \"output\"\n"),
        "unknown source `tmp`",
    );
    assert_error(
        &effect_with(
            "[[effect.texture]]\nname = \"tmp\"\n[[effect.pass]]\nentry = \"a\"\nreads = [\"tmp\"]\nwrites = \"tmp\"\n",
        ),
        "before a pass writes it",
    );
    assert_error(
        &effect_with(
            "[[effect.texture]]\nname = \"tmp\"\n[[effect.pass]]\nentry = \"a\"\nwrites = \"tmp\"\n",
        ),
        "last pass must write `output`",
    );
    assert_error(
        &format!(
            "{HEADER}[[effect]]\nid = \"n\"\nname = \"N\"\nimplementation = \"native\"\ndevices = [\"cpu\"]\n"
        ),
        "need a [native] table",
    );
    assert_error(
        &format!("{HEADER}[[automation]]\nid = \"a\"\nname = \"A\"\nentry = \"run\"\n"),
        "[script] table",
    );
}

#[test]
fn groups_map_to_group_start_and_end() {
    let toml = effect_with(
        "[[effect.param]]\nid = 1\nkey = \"g\"\ntype = \"group\"\nname = \"G\"\nend_id = 4\n\
         [[effect.param]]\nid = 2\nkey = \"a\"\ntype = \"float\"\nname = \"A\"\ngroup = \"g\"\n\
         [[effect.param]]\nid = 3\nkey = \"b\"\ntype = \"checkbox\"\nname = \"B\"\ngroup = \"g\"\n\
         [[effect.param]]\nid = 5\nkey = \"c\"\ntype = \"angle\"\nname = \"C\"\n",
    );
    assert!(errors(&toml).is_empty(), "{:?}", errors(&toml));
    let pkg = Package::load(
        &MemorySource::new("test")
            .with_file("bae-plugin.toml", toml)
            .with_file(
                "fx.wgsl",
                "fn fx(c: vec4<f32>, px: BaePixel) -> vec4<f32> { return c * params.a; }",
            ),
    )
    .unwrap();
    let kinds: Vec<(u32, &ParamKind)> = pkg.effects[0]
        .def
        .params
        .iter()
        .map(|p| (p.id, &p.kind))
        .collect();
    assert!(matches!(kinds[0], (1, ParamKind::GroupStart)));
    assert!(matches!(kinds[3], (4, ParamKind::GroupEnd)));
    assert!(matches!(kinds[4], (5, ParamKind::Angle)));

    let split = effect_with(
        "[[effect.param]]\nid = 1\nkey = \"g\"\ntype = \"group\"\nname = \"G\"\nend_id = 4\n\
         [[effect.param]]\nid = 2\nkey = \"a\"\ntype = \"float\"\nname = \"A\"\n\
         [[effect.param]]\nid = 3\nkey = \"b\"\ntype = \"checkbox\"\nname = \"B\"\ngroup = \"g\"\n",
    );
    assert_error(&split, "contiguously");
}

#[test]
fn missing_files_and_shader_errors_fail_the_load() {
    let toml = effect_with("");
    let errs = Package::load(&MemorySource::new("t").with_file("bae-plugin.toml", toml.clone()))
        .unwrap_err();
    assert!(
        errs.iter()
            .any(|d| d.message.contains("`fx.wgsl` does not exist"))
    );

    let errs = Package::load(
        &MemorySource::new("t")
            .with_file("bae-plugin.toml", toml)
            .with_file(
                "fx.wgsl",
                "fn fx(c: vec4<f32>, px: BaePixel) -> vec4<f32> {\n  return c +;\n}\n",
            ),
    )
    .unwrap_err();
    assert!(has_errors(&errs));
    assert_eq!(errs[0].file, "fx.wgsl");
    assert_eq!(errs[0].line, Some(2));
    assert_eq!(errs[0].context, "effect \"fx\"");
}
