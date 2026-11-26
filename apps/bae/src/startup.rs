//! Process startup: command line, directories, logging, crash reports, then the
//! GUI, headless runner or a subcommand.

use std::process::ExitCode;

use bae_app::App;
use bae_platform::cli::{self, CliSubcommand, Invocation};
use bae_platform::crash::{self, CrashConfig};
use bae_platform::logging::{self, LogOptions};
use bae_platform::{AppDirs, console};

/// Runs the assembled application and returns the process exit code.
pub fn run(mut app: App) -> ExitCode {
    // Windows GUI-subsystem binary started from a terminal: reconnect stdio.
    console::attach_parent_console();

    let registry = app.registry::<CliSubcommand>();
    let invocation = match cli::parse(std::env::args_os(), &registry) {
        Ok(invocation) => invocation,
        Err(error) => {
            // `--help` / `--version` are "errors" that print to stdout and exit 0.
            let _ = error.print();
            return if error.use_stderr() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    if let Invocation::Subcommand { name, matches, .. } = &invocation
        && name == "plugin"
        && matches.subcommand_name() == Some("runner")
    {
        return bae_plugin::native::runner_main();
    }
    let global = invocation.global().clone();
    let is_subcommand = matches!(invocation, Invocation::Subcommand { .. });

    let dirs = match AppDirs::resolve() {
        Ok(dirs) => dirs,
        Err(error) => {
            eprintln!("{}: {error}", bae_platform::BINARY_NAME);
            return ExitCode::FAILURE;
        }
    };
    let dirs_ok = dirs.create_all();

    // Subcommands print results on stdout; keep stderr quiet unless asked, and do
    // not litter the log directory with one file per `bae gpu-info`.
    let quiet_default = is_subcommand
        && std::env::var_os(logging::LOG_ENV).is_none()
        && std::env::var_os("RUST_LOG").is_none();
    let log_options = LogOptions {
        filter: global
            .log
            .clone()
            .or_else(|| quiet_default.then(|| "warn".to_owned())),
        file_dir: (!is_subcommand && dirs_ok.is_ok()).then(|| dirs.logs.clone()),
        ..LogOptions::default()
    };
    let _log_guard = match logging::init(&log_options) {
        Ok(guard) => Some(guard),
        Err(error) => {
            eprintln!("{}: logging disabled: {error}", bae_platform::BINARY_NAME);
            None
        }
    };
    if let Err(error) = dirs_ok {
        tracing::warn!("cannot create application directories: {error}");
    }
    tracing::info!("{}", bae_platform::version_line());

    crash::install(CrashConfig::new(dirs.crash_reports.clone()));
    if !is_subcommand && let Some(report) = crash::take_previous_crash(&dirs.crash_reports) {
        tracing::warn!("the previous session crashed; report: {}", report.display());
    }
    if !is_subcommand {
        bae_ui_motion::register_preset_index(&mut app, dirs.data.join("Presets"));
    }
    app.insert_resource(dirs);
    // Installed plugins (user, system, bundled, BAE_PLUGIN_PATH); problems are logged.
    bae_plugin::load_installed(&app);
    let packages = app
        .resource::<bae_plugin::PluginRuntime>()
        .map(|runtime| {
            runtime.with_host(|host| {
                host.plugins()
                    .filter_map(|plugin| {
                        Some((plugin.id().to_owned(), plugin.package.root.clone()?))
                    })
                    .collect()
            })
        })
        .unwrap_or_default();
    bae_ui_motion::effects::manager::configure(&mut app, packages);

    // Messages of subcommands (e.g. why `bae export-json` cannot open a file)
    // follow the UI language too.
    crate::i18n::init_from_env();
    match invocation {
        Invocation::Subcommand {
            name,
            matches,
            global,
        } => cli::dispatch(&mut app, &name, &matches, &global),
        Invocation::Headless(global) => crate::headless::run(&mut app, &global),
        Invocation::Gui(global) => crate::shell::run(app, &global),
    }
}
