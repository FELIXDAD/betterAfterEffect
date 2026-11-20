//! `bae-hub`: the BAE collaboration hub. See `deploy/README.md` for deployment.

use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use bae_hub::config::{self, DEFAULT_PORT};
use clap::{Parser, Subcommand};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::prelude::*;

#[derive(Parser, Debug)]
#[command(
    name = "bae-hub",
    version,
    about = "BAE collaboration hub (real-time sync for a team, runs on a NAS)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the hub (the default).
    Serve(ServeArgs),
    /// Print a new random token for hub.toml.
    GenToken,
    /// Exit 0 if the hub on this machine answers (Docker HEALTHCHECK).
    Healthcheck {
        /// Address to probe; defaults to the port of BAE_HUB_LISTEN on 127.0.0.1.
        #[arg(long)]
        addr: Option<SocketAddr>,
    },
}

#[derive(clap::Args, Debug, Clone)]
struct ServeArgs {
    /// Data directory (projects and hub.toml).
    #[arg(long, env = "BAE_HUB_DATA", default_value = "bae-hub-data")]
    data: PathBuf,
    /// Configuration file [default: <data>/hub.toml].
    #[arg(long, env = "BAE_HUB_CONFIG")]
    config: Option<PathBuf>,
    /// Address and port to listen on [default: 0.0.0.0:7341, or `listen` in hub.toml].
    #[arg(long, env = "BAE_HUB_LISTEN")]
    listen: Option<SocketAddr>,
    /// Log level: error, warn, info, debug or trace.
    #[arg(long, env = "BAE_HUB_LOG", default_value = "info")]
    log: LevelFilter,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::GenToken) => {
            println!("{}", config::generate_token());
            ExitCode::SUCCESS
        }
        Some(Command::Healthcheck { addr }) => healthcheck(addr),
        Some(Command::Serve(args)) => run(args),
        None => run(cli.serve),
    }
}

fn healthcheck(addr: Option<SocketAddr>) -> ExitCode {
    let port = std::env::var("BAE_HUB_LISTEN")
        .ok()
        .and_then(|s| s.parse::<SocketAddr>().ok())
        .map_or(DEFAULT_PORT, |a| a.port());
    let addr = addr.unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], port)));
    match bae_hub::probe::get(addr, "/healthz", None, Duration::from_secs(3)) {
        Ok(r) if r.status == 200 => ExitCode::SUCCESS,
        Ok(r) => {
            eprintln!("bae-hub at {addr} answered {}", r.status);
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("bae-hub at {addr} is not reachable: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: ServeArgs) -> ExitCode {
    init_logging(args.log);
    match serve(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

fn init_logging(level: LevelFilter) {
    let filter = Targets::new()
        .with_default(LevelFilter::WARN.min(level))
        .with_target("bae_hub", level)
        .with_target("bae_sync", level);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(std::io::stderr().is_terminal())
                .with_target(false),
        )
        .with(filter)
        .init();
}

fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let (mut config, report) = config::load(&args.data, args.config.as_deref(), |k| {
        std::env::var(k).ok()
    })?;
    if let Some(listen) = args.listen {
        config.listen = listen;
    }
    if let Some(token) = &report.generated_token {
        tracing::warn!(
            "no access configured: generated a write token and saved it in {}\n\n    team token: {token}\n\nGive it to the team (BAE: connect to hub). It is shown only once; it stays in that file.",
            report.config_path.display()
        );
    }
    signal::install();
    let hub = bae_hub::start(config)?;
    while !signal::requested() {
        std::thread::sleep(Duration::from_millis(100));
    }
    tracing::info!("shutting down");
    hub.shutdown();
    Ok(())
}

/// SIGTERM (`docker stop`, systemd) and Ctrl-C set a flag the main thread polls.
#[cfg(unix)]
mod signal {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REQUESTED: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_signal(_: libc::c_int) {
        // Only an atomic store: async-signal-safe.
        REQUESTED.store(true, Ordering::SeqCst);
    }

    pub fn install() {
        let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        for sig in [libc::SIGINT, libc::SIGTERM] {
            // SAFETY: `on_signal` is a valid `extern "C"` handler that only does an
            // atomic store, which is async-signal-safe.
            unsafe {
                libc::signal(sig, handler);
            }
        }
    }

    pub fn requested() -> bool {
        REQUESTED.load(Ordering::SeqCst)
    }
}

/// Without Unix signals Ctrl-C ends the process directly (projects are written
/// continuously; clients upload anything missing when they reconnect).
#[cfg(not(unix))]
mod signal {
    pub fn install() {}

    pub fn requested() -> bool {
        false
    }
}
