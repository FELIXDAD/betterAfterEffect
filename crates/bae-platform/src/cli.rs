//! The `bae` command line.
//!
//! ```text
//! bae [PROJECT…]                 open the GUI (optionally with projects)
//! bae --headless                 run without a window (batch / CI)
//! bae --smoke-test <FRAMES>      render FRAMES frames, then exit 0 (GUI or headless)
//! bae --log <FILTER>             log filter, e.g. "info,bae_gpu=debug"
//! bae --version
//! bae gpu-info [--json]          GPU adapters, features, limits
//! bae render | mcp | export-json | pack …   reserved for other workstreams
//! ```
//!
//! Subcommands live in a runtime [`Registry<CliSubcommand>`](bae_app::Registry):
//! [`crate::PlatformModule`] registers the reserved names from
//! [`reserved_subcommands`] as *unimplemented* placeholders (they print
//! "not implemented yet" and exit with [`EXIT_NOT_IMPLEMENTED`]). A workstream
//! implements one with [`implement`] from its `AppModule::build`, and may add
//! entirely new subcommands with `Registry::register`.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use bae_app::{App, Owner, Registry, RegistryError};
use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};

/// Exit code of a reserved subcommand that no module implements yet.
pub const EXIT_NOT_IMPLEMENTED: u8 = 3;

/// Builds the clap arguments of a subcommand.
pub type ArgsFn = Arc<dyn Fn(Command) -> Command + Send + Sync>;
/// Runs a subcommand.
pub type RunFn = Arc<dyn Fn(&mut SubcommandContext<'_>) -> anyhow::Result<ExitCode> + Send + Sync>;

/// Everything a subcommand implementation receives.
pub struct SubcommandContext<'a> {
    /// The assembled application (resources and registries).
    pub app: &'a mut App,
    /// The subcommand's parsed arguments.
    pub matches: &'a ArgMatches,
    /// Global options (`--log`, …).
    pub global: &'a GlobalOptions,
}

/// A CLI subcommand definition (an item of `Registry<CliSubcommand>`).
#[derive(Clone)]
pub struct CliSubcommand {
    name: String,
    about: String,
    args: Option<ArgsFn>,
    run: Option<RunFn>,
    hidden: bool,
}

impl CliSubcommand {
    /// A subcommand without arguments and without implementation.
    pub fn new(name: impl Into<String>, about: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: about.into(),
            args: None,
            run: None,
            hidden: false,
        }
    }

    /// Sets the argument definitions.
    pub fn with_args(mut self, args: impl Fn(Command) -> Command + Send + Sync + 'static) -> Self {
        self.args = Some(Arc::new(args));
        self
    }

    /// Sets the implementation.
    pub fn with_run(
        mut self,
        run: impl Fn(&mut SubcommandContext<'_>) -> anyhow::Result<ExitCode> + Send + Sync + 'static,
    ) -> Self {
        self.run = Some(Arc::new(run));
        self
    }

    /// Hides the subcommand from `--help` (developer tools).
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// Subcommand name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// One-line description.
    pub fn about(&self) -> &str {
        &self.about
    }

    /// `true` if a module provided an implementation.
    pub fn is_implemented(&self) -> bool {
        self.run.is_some()
    }

    fn to_command(&self) -> Command {
        let mut cmd = Command::new(self.name.clone()).about(self.about.clone());
        if self.hidden {
            cmd = cmd.hide(true);
        }
        match &self.args {
            Some(args) => args(cmd),
            None => cmd,
        }
    }
}

impl fmt::Debug for CliSubcommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CliSubcommand")
            .field("name", &self.name)
            .field("implemented", &self.is_implemented())
            .finish_non_exhaustive()
    }
}

/// Accepts any arguments; used by reserved placeholders so `bae render foo.bae`
/// reaches the "not implemented yet" message instead of a parse error.
fn passthrough_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("args")
            .value_name("ARGS")
            .num_args(0..)
            .trailing_var_arg(true)
            .allow_hyphen_values(true)
            .help("Arguments (defined by the implementing workstream)"),
    )
}

/// Subcommand names reserved for other workstreams, with their descriptions.
pub fn reserved_subcommands() -> Vec<CliSubcommand> {
    vec![
        CliSubcommand::new(
            "gpu-info",
            "Print GPU adapters, features, limits and capabilities",
        )
        .with_args(|cmd| {
            cmd.arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Print machine-readable JSON"),
            )
        }),
        CliSubcommand::new("render", "Render a composition without a window")
            .with_args(passthrough_args),
        CliSubcommand::new("mcp", "Run the MCP server for agents (stdio)")
            .with_args(passthrough_args),
        CliSubcommand::new("export-json", "Export a project sublayer as readable JSON")
            .with_args(passthrough_args),
        CliSubcommand::new("pack", "Pack a project and its assets into a .baez file")
            .with_args(passthrough_args),
    ]
}

/// Errors from [`implement`].
#[derive(Debug, thiserror::Error)]
pub enum ImplementError {
    /// Another module already implements this subcommand.
    #[error("subcommand `{name}` is already implemented by {owner}")]
    AlreadyImplemented {
        /// Subcommand name.
        name: String,
        /// Owner of the existing implementation.
        owner: Owner,
    },
    /// The registry rejected the entry.
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

/// Implements (or adds) a subcommand. Replaces an unimplemented placeholder,
/// keeping its description and arguments unless `subcommand` provides its own.
pub fn implement(
    registry: &Registry<CliSubcommand>,
    owner: Owner,
    mut subcommand: CliSubcommand,
) -> Result<(), ImplementError> {
    registry.batch(|tx| {
        let name = subcommand.name.clone();
        if let Some(existing) = tx.get(&name) {
            if existing.is_implemented() {
                let owner = registry
                    .snapshot()
                    .entry(&name)
                    .map(|e| e.owner().clone())
                    .unwrap_or(Owner::Core);
                return Err(ImplementError::AlreadyImplemented { name, owner });
            }
            if subcommand.about.is_empty() {
                subcommand.about = existing.about.clone();
            }
            if subcommand.args.is_none() {
                subcommand.args = existing.args.clone();
            }
        }
        tx.replace(owner, name, Arc::new(subcommand))?;
        Ok(())
    })
}

/// Options shared by every mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalOptions {
    /// `--headless`: no window.
    pub headless: bool,
    /// `--smoke-test <FRAMES>`: render that many frames, then exit.
    pub smoke_test: Option<u32>,
    /// `--log <FILTER>`.
    pub log: Option<String>,
    /// Project paths to open.
    pub open: Vec<PathBuf>,
}

/// What the user asked for.
#[derive(Debug, Clone)]
pub enum Invocation {
    /// Open the main window (default).
    Gui(GlobalOptions),
    /// Run without a window.
    Headless(GlobalOptions),
    /// Run a subcommand.
    Subcommand {
        /// Subcommand name.
        name: String,
        /// Its parsed arguments.
        matches: ArgMatches,
        /// Global options.
        global: GlobalOptions,
    },
}

impl Invocation {
    /// Global options of any invocation.
    pub fn global(&self) -> &GlobalOptions {
        match self {
            Self::Gui(global) | Self::Headless(global) => global,
            Self::Subcommand { global, .. } => global,
        }
    }
}

/// Builds the full clap command from the registered subcommands.
pub fn command<'a>(subcommands: impl IntoIterator<Item = &'a CliSubcommand>) -> Command {
    let mut cmd = Command::new(crate::BINARY_NAME)
        .about("BAE — GPU-first motion graphics and compositing")
        .version(crate::VERSION)
        .long_version(crate::version_details())
        .args_conflicts_with_subcommands(true)
        .arg(
            Arg::new("headless")
                .long("headless")
                .action(ArgAction::SetTrue)
                .help("Run without opening a window"),
        )
        .arg(
            Arg::new("smoke-test")
                .long("smoke-test")
                .value_name("FRAMES")
                .value_parser(value_parser!(u32).range(1..))
                .help("Render FRAMES frames and exit (0 on success)"),
        )
        .arg(
            Arg::new("log")
                .long("log")
                .value_name("FILTER")
                .global(true)
                .help("Log filter, e.g. \"info,bae_gpu=debug\" (overrides BAE_LOG)"),
        )
        .arg(
            Arg::new("paths")
                .value_name("PROJECT")
                .num_args(0..)
                .value_parser(value_parser!(PathBuf))
                .help("Projects to open"),
        );
    for sub in subcommands {
        cmd = cmd.subcommand(sub.to_command());
    }
    cmd
}

/// Parses `args` (including the program name) against the registered subcommands.
pub fn parse<I, T>(args: I, registry: &Registry<CliSubcommand>) -> Result<Invocation, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let snapshot = registry.snapshot();
    let matches = command(snapshot.iter().map(|e| e.item().as_ref())).try_get_matches_from(args)?;
    let log = matches.get_one::<String>("log").cloned();
    let mut global = GlobalOptions {
        headless: matches.get_flag("headless"),
        smoke_test: matches.get_one::<u32>("smoke-test").copied(),
        log,
        open: matches
            .get_many::<PathBuf>("paths")
            .map(|paths| paths.cloned().collect())
            .unwrap_or_default(),
    };
    Ok(match matches.subcommand() {
        Some((name, sub_matches)) => {
            if global.log.is_none() {
                global.log = sub_matches.get_one::<String>("log").cloned();
            }
            Invocation::Subcommand {
                name: name.to_owned(),
                matches: sub_matches.clone(),
                global,
            }
        }
        None if global.headless => Invocation::Headless(global),
        None => Invocation::Gui(global),
    })
}

/// Runs a parsed subcommand. Unimplemented subcommands print
/// "`bae <name>`: not implemented yet" and return [`EXIT_NOT_IMPLEMENTED`];
/// errors are printed to stderr and return exit code 1.
pub fn dispatch(
    app: &mut App,
    name: &str,
    matches: &ArgMatches,
    global: &GlobalOptions,
) -> ExitCode {
    let registry = app.registry::<CliSubcommand>();
    let Some(subcommand) = registry.get(name) else {
        eprintln!("{}: unknown subcommand `{name}`", crate::BINARY_NAME);
        return ExitCode::from(2);
    };
    let Some(run) = subcommand.run.clone() else {
        eprintln!("{} {name}: not implemented yet", crate::BINARY_NAME);
        return ExitCode::from(EXIT_NOT_IMPLEMENTED);
    };
    let mut ctx = SubcommandContext {
        app,
        matches,
        global,
    };
    match run(&mut ctx) {
        Ok(code) => code,
        Err(error) => {
            tracing::error!(subcommand = name, "{error:#}");
            eprintln!("{} {name}: error: {error:#}", crate::BINARY_NAME);
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_app::AppBuilder;

    fn app() -> App {
        let mut builder = AppBuilder::new();
        builder.add_module(crate::PlatformModule);
        builder.build()
    }

    fn parse_args(app: &mut App, args: &[&str]) -> Result<Invocation, clap::Error> {
        let registry = app.registry::<CliSubcommand>();
        parse(
            std::iter::once("bae").chain(args.iter().copied()),
            &registry,
        )
    }

    #[test]
    fn default_is_gui_and_flags_parse() {
        let mut app = app();
        assert!(matches!(
            parse_args(&mut app, &[]).unwrap(),
            Invocation::Gui(_)
        ));
        let inv = parse_args(
            &mut app,
            &["--headless", "--smoke-test", "60", "--log", "debug"],
        )
        .unwrap();
        let Invocation::Headless(global) = inv else {
            panic!("expected headless")
        };
        assert_eq!(global.smoke_test, Some(60));
        assert_eq!(global.log.as_deref(), Some("debug"));
        let inv = parse_args(&mut app, &["a.bae", "b.bae"]).unwrap();
        assert_eq!(inv.global().open.len(), 2);
        assert!(parse_args(&mut app, &["--smoke-test", "0"]).is_err());
    }

    #[test]
    fn reserved_subcommands_parse_and_report_not_implemented() {
        let mut app = app();
        let inv = parse_args(&mut app, &["render", "film.bae", "--comp", "main"]).unwrap();
        let Invocation::Subcommand {
            name,
            matches,
            global,
        } = inv
        else {
            panic!("expected subcommand")
        };
        assert_eq!(name, "render");
        let code = dispatch(&mut app, &name, &matches, &global);
        assert_eq!(code, ExitCode::from(EXIT_NOT_IMPLEMENTED));
    }

    #[test]
    fn global_log_after_subcommand() {
        let mut app = app();
        let inv = parse_args(&mut app, &["gpu-info", "--json", "--log", "warn"]).unwrap();
        assert_eq!(inv.global().log.as_deref(), Some("warn"));
    }

    #[test]
    fn implement_replaces_placeholder_once() {
        let mut app = app();
        let registry = app.registry::<CliSubcommand>();
        let sub = CliSubcommand::new("gpu-info", "").with_run(|ctx| {
            assert!(
                ctx.matches.get_flag("json"),
                "args kept from the placeholder"
            );
            Ok(ExitCode::from(7))
        });
        implement(&registry, Owner::module("gpu"), sub.clone()).unwrap();
        assert!(matches!(
            implement(&registry, Owner::module("other"), sub),
            Err(ImplementError::AlreadyImplemented { .. })
        ));
        let Invocation::Subcommand {
            name,
            matches,
            global,
        } = parse_args(&mut app, &["gpu-info", "--json"]).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            dispatch(&mut app, &name, &matches, &global),
            ExitCode::from(7)
        );
        assert!(
            registry
                .get("gpu-info")
                .unwrap()
                .about()
                .contains("GPU adapters")
        );
    }

    #[test]
    fn modules_can_add_new_subcommands_at_runtime() {
        let mut app = app();
        let registry = app.registry::<CliSubcommand>();
        registry
            .register(
                Owner::plugin("com.example.tool"),
                "hello",
                CliSubcommand::new("hello", "Say hello").with_run(|_| Ok(ExitCode::SUCCESS)),
            )
            .unwrap();
        let Invocation::Subcommand {
            name,
            matches,
            global,
        } = parse_args(&mut app, &["hello"]).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            dispatch(&mut app, &name, &matches, &global),
            ExitCode::SUCCESS
        );
        app.unregister_owner(&Owner::plugin("com.example.tool"));
        assert!(parse_args(&mut app, &["hello"]).is_err() || !registry.contains("hello"));
    }

    #[test]
    fn errors_become_exit_code_one() {
        let mut app = app();
        let registry = app.registry::<CliSubcommand>();
        implement(
            &registry,
            Owner::Core,
            CliSubcommand::new("pack", "").with_run(|_| anyhow::bail!("disk full")),
        )
        .unwrap();
        let Invocation::Subcommand {
            name,
            matches,
            global,
        } = parse_args(&mut app, &["pack"]).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            dispatch(&mut app, &name, &matches, &global),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn version_and_help_render() {
        let mut app = app();
        let err = parse_args(&mut app, &["--version"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
        let help = command(reserved_subcommands().iter())
            .render_help()
            .to_string();
        for name in [
            "gpu-info",
            "render",
            "mcp",
            "export-json",
            "pack",
            "--headless",
            "--smoke-test",
        ] {
            assert!(help.contains(name), "help mentions {name}");
        }
    }
}
