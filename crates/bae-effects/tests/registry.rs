//! Every built-in effect is registered through the plugin registries with an
//! AE-compatible definition and complete catalog metadata (plugin spec §19.6).

use std::collections::HashSet;

use bae_effects::{Form, Implementation, catalog, category_zh};
use bae_model::{EffectRegistry, ParamKind};
use bae_render::EffectLibrary;

#[test]
fn builtins_register_through_the_plugin_registries() {
    let t = std::time::Instant::now();
    let all = bae_effects::builtins();
    eprintln!("loading {} built-ins took {:?}", all.len(), t.elapsed());
    let mut defs = EffectRegistry::new();
    let mut library = EffectLibrary::new();
    let n = bae_effects::register(&mut defs, &mut library);
    assert_eq!(n, all.len());
    assert_eq!(library.len(), n);
    assert_eq!(defs.iter().count(), n);
    let mut ids = HashSet::new();
    for b in all {
        let def = b.def();
        assert_eq!(def.match_name, b.info.match_name);
        assert!(defs.get(&def.match_name).is_some());
        assert!(library.get(&def.match_name).is_some());
        assert!(
            ids.insert(b.info.full_id.clone()),
            "duplicate {}",
            b.info.full_id
        );
        assert!(b.info.full_id.starts_with("bae.effects/"));
        // Built-ins are not plugin effects: never removed, never "missing".
        assert!(def.plugin.is_none(), "{}", def.match_name);
        assert!(!def.match_name.is_empty());
        assert!(
            !b.info.name.is_empty() && !b.info.name_zh.is_empty(),
            "{}",
            def.match_name
        );
        assert_ne!(category_zh(&b.info.category), "其他", "{}", b.info.category);
        assert!(
            b.info.keywords.iter().any(|k| !k.is_ascii()),
            "no Chinese keyword: {}",
            def.match_name
        );
        let mut pids = HashSet::new();
        for p in &def.params {
            assert!(
                pids.insert(p.id),
                "{}: duplicate param id {}",
                def.match_name,
                p.id
            );
            if !matches!(p.kind, ParamKind::GroupEnd) {
                assert!(
                    b.info.param_name_zh(p.id).is_some(),
                    "{}: param {} ({}) has no Chinese name",
                    def.match_name,
                    p.id,
                    p.name
                );
            }
        }
        match (&b.implementation, b.info.form) {
            (Implementation::Declarative(_), Form::DeclarativePixel | Form::DeclarativeCompute)
            | (Implementation::Rust(_), Form::Rust) => {}
            other => panic!("{}: form mismatch {other:?}", def.match_name),
        }
    }
    // Lookup and search.
    let blur = bae_effects::get("ADBE Gaussian Blur 2").unwrap();
    assert_eq!(blur.info.name_zh, "高斯模糊");
    assert!(catalog().any(|i| i.matches("模糊")));
    assert!(catalog().any(|i| i.matches("gaussian")));
}

#[test]
fn ae_parameter_ids_and_defaults() {
    let get = |m: &str| bae_effects::get(m).unwrap().def().clone();
    let g = get("ADBE Gaussian Blur 2");
    assert_eq!(g.params.iter().map(|p| p.id).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(
        g.params[0].match_name.as_deref(),
        Some("ADBE Gaussian Blur 2-0001")
    );
    let levels = get("ADBE Pro Levels2");
    // Group start and end markers consume ids like in AE.
    let red_gamma = levels.param(13).unwrap();
    assert_eq!(red_gamma.name, "Red Gamma");
    assert!(matches!(
        levels.param(10).unwrap().kind,
        ParamKind::GroupStart
    ));
    assert!(matches!(
        levels.param(16).unwrap().kind,
        ParamKind::GroupEnd
    ));
    let exposure = get("ADBE Exposure2");
    assert_eq!(
        exposure.param(22).unwrap().name,
        "Bypass Linear Light Conversion"
    );
}

#[test]
fn prewarm_compiles_every_pipeline() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    for format in [
        bae_gpu::wgpu::TextureFormat::Rgba16Float,
        bae_gpu::wgpu::TextureFormat::Rgba32Float,
    ] {
        let t = std::time::Instant::now();
        bae_effects::prewarm(&gpu, format).unwrap();
        eprintln!("prewarm {format:?}: {:?}", t.elapsed());
    }
}

#[path = "registry/ae_reference.rs"]
mod ae_reference;

#[test]
fn builtin_defs_follow_the_ae_26_reference() {
    ae_reference::check();
}
