//! The plugin host as an app module (ARCHITECTURE §3, spec §6, §7).
//!
//! * [`PluginModule`] creates the [`PluginRuntime`] resource and implements
//!   `bae plugin check <DIR>...` and `bae plugin list [--json]`.
//! * [`load_installed`] scans the plugin directories and loads the active
//!   plugins. The app calls it once at startup, after the directories are
//!   resolved and logging is up, so problems with installed plugins are logged.
//!
//! Plugin effects are published in the app's runtime registries
//! `Registry<bae_model::EffectDef>` and `Registry<dyn bae_render::Effect>`
//! (id = match name, owner = `Owner::Plugin(<plugin-id>)`), next to the
//! built-in effects that modules register there. Renderers, the UI and the
//! agent tools read those registries; unloading a plugin removes exactly its
//! entries.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bae_app::{App, AppBuilder, AppModule};
use bae_model::EffectDef;
use bae_platform::AppDirs;
use bae_platform::cli::{self, CliSubcommand};
use bae_render::Effect;
use clap::{Arg, ArgAction, ArgMatches, Command};
use parking_lot::Mutex;

use crate::diag::Diagnostic;
use crate::host::{AppEffectRegistries, LoadError, PluginDir, PluginHost, Scope, UnloadReport};
use crate::package::{DirSource, Package, PackageSource};

/// Extra plugin directories (`:`/`;`-separated like `PATH`), searched with
/// project scope; for plugin development.
pub const PLUGIN_PATH_ENV: &str = "BAE_PLUGIN_PATH";

/// The running plugin host (an app resource).
#[derive(Debug)]
pub struct PluginRuntime {
    host: Mutex<PluginHost>,
    registries: AppEffectRegistries,
    problems: Mutex<Vec<(PathBuf, Vec<Diagnostic>)>>,
}

impl PluginRuntime {
    fn new(registries: AppEffectRegistries, host: PluginHost) -> Self {
        Self {
            host: Mutex::new(host),
            registries,
            problems: Mutex::new(Vec::new()),
        }
    }

    /// The registries plugin effects are published in.
    pub fn registries(&self) -> &AppEffectRegistries {
        &self.registries
    }

    /// Runs `f` with the host (loaded plugins, their warnings and effects).
    pub fn with_host<R>(&self, f: impl FnOnce(&PluginHost) -> R) -> R {
        f(&self.host.lock())
    }

    /// Problems found by the last [`PluginRuntime::load_dirs`], by package path.
    pub fn problems(&self) -> Vec<(PathBuf, Vec<Diagnostic>)> {
        self.problems.lock().clone()
    }

    /// Scans `dirs`, selects the active versions and loads them.
    pub fn load_dirs(&self, dirs: &[PluginDir]) -> Vec<String> {
        let mut target = self.registries.clone();
        let (loaded, problems) = self.host.lock().load_dirs(dirs, &mut target);
        *self.problems.lock() = problems;
        loaded
    }

    /// Loads (or updates) one package without restarting; returns its id.
    pub fn load(&self, source: &dyn PackageSource, scope: Scope) -> Result<String, LoadError> {
        let mut target = self.registries.clone();
        let mut host = self.host.lock();
        host.load_from(source, scope, &mut target)
            .map(|p| p.id().to_owned())
    }

    /// Unloads a plugin; its effects in open projects become placeholders.
    pub fn unload(&self, id: &str) -> Option<UnloadReport> {
        let mut target = self.registries.clone();
        self.host.lock().unload(id, &mut target)
    }
}

/// The plugin search directories: user, system and bundled (from `dirs`), plus
/// [`PLUGIN_PATH_ENV`] entries with project scope.
pub fn plugin_dirs(dirs: &AppDirs) -> Vec<PluginDir> {
    let mut out: Vec<PluginDir> = std::env::var_os(PLUGIN_PATH_ENV)
        .map(|v| {
            std::env::split_paths(&v)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| PluginDir::new(p, Scope::Project))
                .collect()
        })
        .unwrap_or_default();
    out.extend(dirs.plugin_dirs().into_iter().map(|d| {
        let scope = match d.scope {
            bae_platform::PluginScope::User => Scope::User,
            bae_platform::PluginScope::System => Scope::System,
            bae_platform::PluginScope::Bundled => Scope::Bundled,
        };
        PluginDir::new(d.path, scope)
    }));
    out
}

/// Loads the installed plugins into the app's registries. Call once at startup
/// after `AppDirs` is in the resources and logging is initialized; problems are
/// logged and kept in [`PluginRuntime::problems`], never fatal.
pub fn load_installed(app: &App) {
    let (Some(runtime), Some(dirs)) = (app.resource::<PluginRuntime>(), app.resource::<AppDirs>())
    else {
        return;
    };
    let dirs = plugin_dirs(dirs);
    let loaded = runtime.load_dirs(&dirs);
    for (path, diags) in runtime.problems() {
        for d in diags {
            tracing::warn!("plugin {}: {d}", path.display());
        }
    }
    runtime.with_host(|host| {
        for p in host.plugins() {
            for w in &p.warnings {
                tracing::info!("plugin {}: {w}", p.id());
            }
        }
    });
    if !loaded.is_empty() {
        tracing::info!("plugins loaded: {}", loaded.join(", "));
    }
}

/// App module of the plugin host.
#[derive(Debug, Default)]
pub struct PluginModule;

impl AppModule for PluginModule {
    fn name(&self) -> &str {
        "bae-plugin"
    }

    fn build(&self, app: &mut AppBuilder) {
        let registry = app.registry::<CliSubcommand>();
        let plugin = CliSubcommand::new("plugin", "Check and list plugins")
            .with_args(args)
            .with_run(|ctx| {
                let dirs = ctx
                    .app
                    .resource::<AppDirs>()
                    .map(plugin_dirs)
                    .unwrap_or_default();
                match ctx.matches.subcommand() {
                    Some(("check", m)) => check(m),
                    Some(("list", m)) => {
                        let runtime = ctx.app.resource::<PluginRuntime>();
                        list(m, &dirs, runtime)
                    }
                    _ => Ok(ExitCode::from(2)),
                }
            });
        if let Err(error) = cli::implement(&registry, app.current_owner(), plugin) {
            tracing::warn!("plugin subcommand not registered: {error}");
        }
    }

    fn finish(&self, app: &mut AppBuilder) {
        let defs = app.registry::<EffectDef>();
        let view = app
            .resource::<std::sync::Arc<dyn bae_script::ScriptView>>()
            .cloned();
        let scripts = crate::scripts::ScriptCommands::new(
            app.registry::<dyn bae_model::DynCommand>(),
            defs.clone(),
            view,
        );
        let bindings = app
            .resource::<std::sync::Arc<dyn crate::scripts::ScriptBindings>>()
            .cloned();
        let runtime = PluginRuntime::new(
            AppEffectRegistries {
                definitions: defs,
                effects: app.registry::<dyn Effect>(),
            },
            PluginHost::with_scripts(scripts, bindings),
        );
        app.insert_resource(runtime);
    }
}

fn args(cmd: Command) -> Command {
    cmd.subcommand_required(true)
        .subcommand(
            Command::new("check")
                .about("Validate plugin packages like the host does (manifest, files, WGSL)")
                .arg(
                    Arg::new("dirs")
                        .value_name("DIR")
                        .required(true)
                        .num_args(1..)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Package directories (containing bae-plugin.toml)"),
                ),
        )
        .subcommand(
            Command::new("list")
                .about("List the plugins found in the plugin directories")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help("Print machine-readable JSON"),
                ),
        )
}

/// `bae plugin check`: exit 0 when every package loads (warnings allowed).
fn check(m: &ArgMatches) -> anyhow::Result<ExitCode> {
    let mut ok = true;
    for dir in m.get_many::<PathBuf>("dirs").into_iter().flatten() {
        let shown = dir.display();
        match Package::load(&DirSource::new(dir)) {
            Ok(p) => {
                for w in &p.warnings {
                    println!("{shown}/{w}");
                }
                let effects: Vec<&str> = p
                    .effects
                    .iter()
                    .map(|e| e.def.match_name.as_str())
                    .collect();
                println!(
                    "{shown}: ok — {} {} (API {}), effects: {}",
                    p.id(),
                    p.version(),
                    p.manifest.plugin.api,
                    if effects.is_empty() {
                        "none".to_owned()
                    } else {
                        effects.join(", ")
                    }
                );
            }
            Err(diags) => {
                ok = false;
                for d in &diags {
                    println!("{shown}/{d}");
                }
                println!("{shown}: failed ({} problem(s))", diags.len());
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// One row of `bae plugin list`.
#[derive(Debug, serde::Serialize)]
struct Row {
    id: String,
    version: String,
    scope: &'static str,
    path: PathBuf,
    /// `loaded`, `active` (selected but not loaded in this process),
    /// `inactive` (another version wins) or `failed`.
    status: &'static str,
    effects: Vec<String>,
    problems: Vec<String>,
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Bundled => "bundled",
        Scope::System => "system",
        Scope::User => "user",
        Scope::Project => "project",
    }
}

/// `bae plugin list`.
fn list(
    m: &ArgMatches,
    dirs: &[PluginDir],
    runtime: Option<&PluginRuntime>,
) -> anyhow::Result<ExitCode> {
    let report = PluginHost::scan(dirs);
    let active: Vec<&Path> = PluginHost::select(&report.candidates)
        .into_iter()
        .map(|c| c.path.as_path())
        .collect();
    let load_problems = runtime.map(PluginRuntime::problems).unwrap_or_default();
    // (id, package location) of what the runtime actually loaded.
    let loaded: Vec<(String, String)> = runtime
        .map(|r| {
            r.with_host(|h| {
                h.plugins()
                    .map(|p| (p.id().to_owned(), p.package.location.clone()))
                    .collect()
            })
        })
        .unwrap_or_default();
    let mut rows: Vec<Row> = report
        .candidates
        .iter()
        .map(|c| {
            let problems: Vec<String> = load_problems
                .iter()
                .filter(|(p, _)| p == &c.path)
                .flat_map(|(_, d)| d.iter().map(ToString::to_string))
                .collect();
            let location = c.path.display().to_string();
            let status = if !problems.is_empty() {
                "failed"
            } else if loaded
                .iter()
                .any(|(id, l)| *id == c.manifest.plugin.id && *l == location)
            {
                "loaded"
            } else if active.contains(&c.path.as_path()) {
                "active"
            } else {
                "inactive"
            };
            Row {
                id: c.manifest.plugin.id.clone(),
                version: c.manifest.plugin.version.to_string(),
                scope: scope_name(c.scope),
                path: c.path.clone(),
                status,
                effects: c
                    .manifest
                    .effects
                    .iter()
                    .map(|e| c.manifest.effect_full_id(e))
                    .collect(),
                problems,
            }
        })
        .collect();
    rows.extend(report.failed.iter().map(|(path, diags)| Row {
        id: String::new(),
        version: String::new(),
        scope: "",
        path: path.clone(),
        status: "failed",
        effects: Vec::new(),
        problems: diags.iter().map(ToString::to_string).collect(),
    }));
    rows.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));

    if m.get_flag("json") {
        let json = serde_json::json!({
            "dirs": dirs.iter().map(|d| serde_json::json!({
                "path": d.path, "scope": scope_name(d.scope)
            })).collect::<Vec<_>>(),
            "plugins": rows,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(ExitCode::SUCCESS);
    }
    println!("Plugin directories:");
    for d in dirs {
        let exists = if d.path.is_dir() { "" } else { " (missing)" };
        println!("  {:<8} {}{exists}", scope_name(d.scope), d.path.display());
    }
    if rows.is_empty() {
        println!("No plugins installed.");
        return Ok(ExitCode::SUCCESS);
    }
    println!();
    for r in &rows {
        if r.id.is_empty() {
            println!("{:<9} {}", r.status, r.path.display());
        } else {
            println!(
                "{:<9} {} {} [{}] {}",
                r.status,
                r.id,
                r.version,
                r.scope,
                r.path.display()
            );
        }
        for e in &r.effects {
            println!("            effect {e}");
        }
        for p in &r.problems {
            println!("            {p}");
        }
    }
    Ok(ExitCode::SUCCESS)
}
