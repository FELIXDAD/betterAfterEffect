//! BAE platform layer.
//!
//! Everything that touches the operating system outside of graphics lives here, so
//! that the rest of the code base stays portable:
//!
//! * [`dirs`] — per-user config / data / cache / log directories and the user,
//!   system and bundled plugin directories.
//! * [`logging`] — `tracing` setup: stderr + a per-session log file with pruning
//!   and size-based rotation, filtered by `BAE_LOG` / `RUST_LOG` / `--log`.
//! * [`crash`] — panic hook that writes a crash report and runs registered
//!   emergency-save callbacks so the user's project survives a crash.
//! * [`single_instance`] — one GUI instance per user.
//! * [`settings`] — versioned JSON settings files with migration and atomic writes.
//! * [`cli`] — the `bae` command line with an extensible subcommand registry.
//! * [`console`] — Windows console attachment for the GUI-subsystem binary.

pub mod cli;
pub mod console;
pub mod crash;
pub mod dirs;
pub mod logging;
pub mod memory;
pub mod settings;
pub mod single_instance;
mod time;

pub use cli::{CliSubcommand, GlobalOptions, Invocation};
pub use dirs::{AppDirs, PluginDir, PluginScope};
pub use settings::{SettingsFile, VersionedSettings};
pub use single_instance::SingleInstance;

/// Product name shown to users.
pub const APP_NAME: &str = "BAE";
/// Executable / CLI name.
pub const BINARY_NAME: &str = "bae";
/// Reverse-DNS application identifier (macOS bundle id, Windows AppUserModelID).
pub const APP_ID: &str = "io.github.felixdad.bae";
/// Application version (workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One-line version string for logs and crash reports, e.g.
/// `BAE 0.1.0 (aarch64-macos, release)`.
/// Shows `path` selected in Finder / Explorer (other systems: its folder).
pub fn reveal(path: &std::path::Path) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
    } else if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()
    };
    if let Err(error) = result {
        tracing::warn!("could not reveal {}: {error}", path.display());
    }
}

pub fn version_line() -> String {
    format!("{APP_NAME} {}", version_details())
}

/// Version with build details, e.g. `0.1.0 (aarch64-macos, release)`; clap
/// prefixes it with the binary name for `bae --version`.
pub fn version_details() -> String {
    format!(
        "{VERSION} ({}-{}, {})",
        std::env::consts::ARCH,
        std::env::consts::OS,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    )
}

/// The platform app module: registers the reserved CLI subcommands
/// (see [`cli::reserved_subcommands`]) so other modules can implement them.
#[derive(Debug, Default)]
pub struct PlatformModule;

impl bae_app::AppModule for PlatformModule {
    fn name(&self) -> &str {
        "bae-platform"
    }

    fn build(&self, app: &mut bae_app::AppBuilder) {
        let registry = app.registry::<CliSubcommand>();
        let owner = app.current_owner();
        registry.batch(|tx| {
            for subcommand in cli::reserved_subcommands() {
                let name = subcommand.name().to_owned();
                // Implementations registered earlier (module order) win.
                if !tx.contains(&name) {
                    tx.register(owner.clone(), name, subcommand)
                        .expect("reserved subcommand names are valid ids");
                }
            }
        });
    }
}
