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

#[test]
fn grid_plugin_runs_both_exports_in_the_callers_transaction() {
    use bae_model::{AuthorId, CommandRegistry, DynCommand, PrimPath, Stage, Time};
    use bae_script::{RunScript, ScriptSelection, ScriptView};
    use serde_json::json;
    use std::sync::Arc;

    #[derive(Debug)]
    struct View(PrimPath, PrimPath, std::sync::Mutex<Time>);
    impl ScriptView for View {
        fn time(&self, _: &PrimPath) -> Time {
            *self.2.lock().unwrap()
        }
        fn set_time(&self, _: &bae_motion::query::CompInfo, time: Time) {
            *self.2.lock().unwrap() = time;
        }

        fn selection(&self) -> ScriptSelection {
            ScriptSelection {
                comp: Some(self.0.clone()),
                layers: vec![self.1.clone()],
                time: self.time(&self.0),
            }
        }
    }

    let mut stage = Stage::new();
    let author = AuthorId::new("user:artist").unwrap();
    let mut setup = CommandRegistry::new();
    setup
        .register(RunScript::standalone(Arc::default()))
        .unwrap();
    let report = setup.execute(&mut stage, &author, "script.run", json!({"source":
        "var c=app.project.items.addComp('Test',100,100,1,2,25); var l=c.layers.addNull(); l.name='Guide'; l.position.setValue([13,23]); [c,l];"
    })).unwrap().output;
    assert!(report["error"].is_null(), "{report}");
    let paths = &report["result"];
    let comp = PrimPath::new(paths[0]["item"].as_str().unwrap()).unwrap();
    let layer = PrimPath::new(paths[1]["layer"].as_str().unwrap()).unwrap();
    let mut builder = AppBuilder::new();
    builder.add_module(bae_motion::MotionModule);
    let view = Arc::new(View(comp.clone(), layer, Default::default()));
    builder.insert_resource::<Arc<dyn ScriptView>>(view.clone());
    builder.add_module(bae_script::ScriptModule);
    builder.add_module(PluginModule);
    let app = builder.build();
    let runtime = app.resource::<PluginRuntime>().unwrap();
    let package = DirSource::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/examples/align-to-grid"),
    );
    let id = runtime.load(&package, Scope::User).unwrap();
    let commands = app.get_registry::<dyn DynCommand>().unwrap();
    let command = "plugin.com.example.align-to-grid.align-to-grid";
    let automation = "script.com.example.align-to-grid.report-off-grid";
    let run = commands.get("script.run").unwrap();
    let (report, _) = stage
        .transact(&author, "Playhead", |tx| {
            run.run_json(
                tx,
                json!({
                    "source": "app.project.activeItem.time = 0.405; app.project.activeItem.time"
                }),
            )
        })
        .unwrap();
    assert!(report["error"].is_null(), "{report}");
    assert_eq!(view.time(&comp), Time::from_seconds(0.4));

    let package = bae_plugin::MemorySource::new("permissions")
        .with_file("bae-plugin.toml", r#"
schema_version = 1
[plugin]
id = "org.test.permissions"
version = "1.0.0"
api = "0.1"
name = "Permissions"
[permissions]
project = "write"
[script]
entry = "main.js"
[[automation]]
id = "invoke"
name = "Invoke"
entry = "invoke"
"#)
        .with_file("main.js", "import { bae } from 'bae'; export function invoke({ id, params }) { return bae.commands.invoke(id, params); }");
    let installed = runtime.load(&package, Scope::User).unwrap();
    // Resolve a command published after installation through the application registry.
    let registry = CommandRegistry::with_builtins();
    commands
        .register_arc(
            Owner::Core,
            "test.rename",
            registry.get("attr.set").unwrap().clone(),
        )
        .unwrap();
    let dispatch = commands.get("script.org.test.permissions.invoke").unwrap();
    let call = |id: &str, params| serde_json::json!({"id": id, "params": params});
    for id in ["item.import", "asset.set_file"] {
        let error = stage
            .transact(&author, "Permissions", |tx| {
                dispatch.run_json(tx, call(id, json!({})))
            })
            .unwrap_err();
        assert!(error.to_string().contains("file handle"), "{error}");
    }
    stage
        .transact(&author, "Rename comp", |tx| {
            dispatch.run_json(
                tx,
                call(
                    "test.rename",
                    json!({
                        "path": comp, "attr": "name", "value":{"type":"string","value":"Renamed"}
                    }),
                ),
            )
        })
        .unwrap();
    runtime.unload(&installed).unwrap();

    let snapshot = commands.snapshot();
    assert_eq!(
        snapshot.entry(command).unwrap().owner(),
        &Owner::plugin(id.as_str())
    );
    assert_eq!(
        commands.get(command).unwrap().info().params_schema["properties"]["size"]["exclusiveMinimum"],
        0
    );
    let invoke = |stage: &mut Stage, id: &str| {
        stage
            .transact(&author, id, |tx| {
                commands.get(id).unwrap().run_json(tx, json!({}))
            })
            .unwrap()
            .0
    };
    assert_eq!(invoke(&mut stage, automation), json!({"layers":["Guide"]}));
    assert_eq!(invoke(&mut stage, command), json!({"moved":1}));
    assert_eq!(invoke(&mut stage, automation), json!({"layers":[]}));
    stage.undo(&author).unwrap();
    assert_eq!(invoke(&mut stage, automation), json!({"layers":["Guide"]}));
    for (example, entry, expected) in [
        (
            "composition-report",
            "script.com.example.composition-report.summary",
            json!({"name":"Renamed","layers":1,"duration":2,"frameRate":25}),
        ),
        (
            "fade-selected",
            "plugin.com.example.fade-selected.fade",
            json!({"animated":1}),
        ),
    ] {
        let source = DirSource::new(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../plugins/examples")
                .join(example),
        );
        let installed = runtime.load(&source, Scope::User).unwrap();
        assert_eq!(invoke(&mut stage, entry), expected);
        runtime.unload(&installed).unwrap();
    }
    runtime.unload(&id).unwrap();
    assert!(commands.get(command).is_none() && commands.get(automation).is_none());
}

#[test]
fn read_only_script_and_invalid_replacement_preserve_the_project_and_active_command() {
    use bae_model::{AuthorId, DynCommand, Stage};
    use bae_plugin::MemorySource;
    use serde_json::json;
    let package = |schema: &str| {
        MemorySource::new("test")
        .with_file("bae-plugin.toml", r#"
schema_version = 1
[plugin]
id = "org.test.reader"
version = "1.0.0"
api = "0.1"
name = "Reader"
[script]
entry = "main.js"
[[command]]
id = "run"
name = "Run"
implementation = "script"
entry = "run"
schema = "args.json"
"#)
        .with_file("args.json", schema)
        .with_file("main.js", "import { app } from 'bae'; export function run() { app.project.items.addFolder('X'); }")
    };
    let mut builder = AppBuilder::new();
    builder.add_module(PluginModule);
    let app = builder.build();
    let runtime = app.resource::<PluginRuntime>().unwrap();
    let deferred = package("{}").with_file(
        "main.js",
        "throw new Error('installation executed top-level code'); function work() {} export { work as run };",
    );
    runtime.load(&deferred, Scope::User).unwrap();
    runtime
        .load(&package(r#"{"type":"object"}"#), Scope::User)
        .unwrap();
    assert!(runtime.load(&package("{"), Scope::User).is_err());
    let cmd = app
        .get_registry::<dyn DynCommand>()
        .unwrap()
        .get("plugin.org.test.reader.run")
        .unwrap();
    let replacement = package("{}").with_file(
        "main.js",
        "throw new Error('installation executed top-level code'); export function other() {}",
    );
    let error = runtime.load(&replacement, Scope::User).unwrap_err();
    assert!(error.to_string().contains("run"), "{error}");
    let active = app
        .get_registry::<dyn DynCommand>()
        .unwrap()
        .get(&cmd.info().id)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&cmd, &active));
    let mut stage = Stage::new();
    let author = AuthorId::new("user:artist").unwrap();
    let err = stage
        .transact(&author, "Run", |tx| cmd.run_json(tx, json!({})))
        .unwrap_err();
    assert!(err.to_string().contains("write permission"), "{err}");
    assert!(!stage.can_undo(&author));
}

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
