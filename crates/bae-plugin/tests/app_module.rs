//! The plugin host inside the app: `PluginModule`, `load_installed`, the
//! runtime registries and `bae plugin` subcommands.

use std::path::{Path, PathBuf};

use bae_app::{AppBuilder, Owner};
use bae_model::EffectDef;
use bae_platform::AppDirs;
use bae_platform::cli::CliSubcommand;
use bae_plugin::host::{LoadError, Scope};
use bae_plugin::{DirSource, PluginModule, PluginRuntime, load_installed};
use bae_render::Effect;

fn write_package(root: &Path, id: &str, version: &str) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("bae-plugin.toml"),
        format!(
            "schema_version = 1\n[plugin]\nid = \"{id}\"\nversion = \"{version}\"\napi = \"0.1\"\nname = \"T\"\n\
             [[effect]]\nid = \"fx\"\nname = \"Fx\"\nimplementation = \"wgsl\"\nshader = \"fx.wgsl\"\n"
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("fx.wgsl"),
        "fn fx(c: vec4<f32>, px: BaePixel) -> vec4<f32> { return c; }\n",
    )
    .unwrap();
    root.to_owned()
}

fn builtin_def(match_name: &str) -> EffectDef {
    EffectDef {
        match_name: match_name.into(),
        name: "Built-in".into(),
        category: String::new(),
        plugin: None,
        params: Vec::new(),
        thread_safe: true,
        gpu: true,
    }
}

#[test]
fn installed_plugins_are_published_in_the_app_registries() {
    let home = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under_root(home.path());
    write_package(
        &dirs.user_plugins.join("com.example.a/1.0.0"),
        "com.example.a",
        "1.0.0",
    );
    write_package(
        &dirs.user_plugins.join("com.example.a/1.1.0"),
        "com.example.a",
        "1.1.0",
    );
    // A plugin that tries to take a built-in effect's id.
    write_package(
        &dirs.user_plugins.join("clash"),
        "com.example.clash",
        "1.0.0",
    );
    // A broken one.
    std::fs::create_dir_all(dirs.user_plugins.join("broken")).unwrap();
    std::fs::write(dirs.user_plugins.join("broken/bae-plugin.toml"), "nope").unwrap();

    let mut builder = AppBuilder::new();
    builder
        .add_module(bae_platform::PlatformModule)
        .add_module(PluginModule);
    // A built-in module registered the same match name first.
    builder
        .registry::<EffectDef>()
        .register(
            Owner::module("bae-effects"),
            "com.example.clash/fx",
            builtin_def("com.example.clash/fx"),
        )
        .unwrap();
    let mut app = builder.build();
    app.insert_resource(dirs);
    load_installed(&app);

    let defs = app.registry::<EffectDef>();
    let effects = app.registry::<dyn Effect>();
    let def = defs
        .get("com.example.a/fx")
        .expect("plugin effect definition");
    assert_eq!(def.plugin.as_ref().unwrap().version, "1.1.0");
    assert!(effects.get("com.example.a/fx").is_some());
    let snapshot = effects.snapshot();
    assert_eq!(
        snapshot.entry("com.example.a/fx").unwrap().owner(),
        &Owner::plugin("com.example.a")
    );
    // The built-in keeps its id; the clash and the broken package are reported.
    assert_eq!(defs.get("com.example.clash/fx").unwrap().name, "Built-in");
    let runtime = app.resource::<PluginRuntime>().unwrap();
    let problems = runtime.problems();
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(problems.iter().any(|(_, d)| {
        d[0].message
            .contains("already registered by module:bae-effects")
    }));

    // Runtime unload removes exactly the plugin's entries; reload brings them back.
    let report = runtime.unload("com.example.a").unwrap();
    assert_eq!((report.definitions_removed, report.effects_removed), (1, 1));
    assert!(defs.get("com.example.a/fx").is_none() && effects.get("com.example.a/fx").is_none());
    let pkg = app
        .resource::<AppDirs>()
        .unwrap()
        .user_plugins
        .join("com.example.a/1.0.0");
    let id = runtime.load(&DirSource::new(pkg), Scope::User).unwrap();
    assert_eq!(id, "com.example.a");
    assert_eq!(
        defs.get("com.example.a/fx")
            .unwrap()
            .plugin
            .as_ref()
            .unwrap()
            .version,
        "1.0.0"
    );
    // App::unregister_owner (generic plugin removal) also reaches plugin effects.
    let removed = app.unregister_owner(&Owner::plugin("com.example.a"));
    assert!(removed >= 2);
    assert!(effects.get("com.example.a/fx").is_none());
    assert!(matches!(
        runtime.load(&DirSource::new(home.path().join("missing")), Scope::User),
        Err(LoadError::Invalid(_))
    ));
}

#[test]
fn plugin_subcommands_run() {
    let home = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under_root(home.path());
    write_package(&dirs.user_plugins.join("a"), "com.example.a", "1.0.0");
    let mut builder = AppBuilder::new();
    builder
        .add_module(bae_platform::PlatformModule)
        .add_module(PluginModule);
    let mut app = builder.build();
    app.insert_resource(dirs.clone());
    load_installed(&app);

    let registry = app.registry::<CliSubcommand>();
    let examples =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/examples/posterize");
    let run = |app: &mut bae_app::App, args: &[&str]| {
        let mut argv = vec!["bae".to_owned()];
        argv.extend(args.iter().map(|s| s.to_string()));
        let invocation = bae_platform::cli::parse(argv, &registry).unwrap();
        let bae_platform::cli::Invocation::Subcommand {
            name,
            matches,
            global,
        } = invocation
        else {
            panic!("not a subcommand")
        };
        bae_platform::cli::dispatch(app, &name, &matches, &global)
    };
    assert_eq!(
        run(&mut app, &["plugin", "check", examples.to_str().unwrap()]),
        std::process::ExitCode::SUCCESS
    );
    assert_eq!(
        run(
            &mut app,
            &["plugin", "check", home.path().to_str().unwrap()]
        ),
        std::process::ExitCode::FAILURE
    );
    assert_eq!(
        run(&mut app, &["plugin", "list", "--json"]),
        std::process::ExitCode::SUCCESS
    );
    assert_eq!(
        run(&mut app, &["plugin", "list"]),
        std::process::ExitCode::SUCCESS
    );
}
