//! BAE desktop application entry point: **assembly only**.
//!
//! Every feature lives in a crate that contributes an [`bae_app::AppModule`];
//! adding a feature means adding one `add_module` line below (or, better, adding
//! it as a dependency of an existing module). The shell itself (window, GPU,
//! egui, menus) is in [`shell`]; startup (logging, crash reports, command line)
//! in [`startup`]. See `docs/spec/app-shell.md`.

// Release builds on Windows are GUI-subsystem executables (no console window on
// double-click); `bae_platform::console` re-attaches to a parent terminal.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod headless;
mod i18n;
mod mcp_cli;
mod model_cli;
mod shell;
mod startup;
mod ui;

use std::process::ExitCode;

use bae_app::AppBuilder;

fn main() -> ExitCode {
    let mut builder = AppBuilder::new();
    builder
        .add_module(bae_platform::PlatformModule)
        .add_module(bae_gpu::GpuModule)
        .add_module(model_cli::ModelCliModule)
        .add_module(mcp_cli::McpCliModule)
        .add_module(bae_render::RenderModule)
        .add_module(shell::ShellModule);
    // Feature modules from other workstreams are added here.
    startup::run(builder.build())
}
