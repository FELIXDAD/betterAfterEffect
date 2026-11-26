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

// 0.0.1 acceptance suite (macOS run): see `acceptance/mod.rs`. The harness
// builds everywhere: the AE shortcut sweep runs on both CI platforms.
#[cfg(test)]
mod acceptance;
mod agent_feed;
mod autosave;
mod document;
mod file_error;
mod files;
mod headless;
mod i18n;
mod mcp_cli;
mod model_cli;
mod pack_cli;
#[cfg(test)]
mod save_roundtrip;
mod shell;
mod startup;
mod ui;
#[cfg(test)]
mod ui_text;
mod view_state;

use std::process::ExitCode;

use bae_app::AppBuilder;

fn main() -> ExitCode {
    if std::env::args_os()
        .skip(1)
        .eq(["plugin", "runner"].map(std::ffi::OsString::from))
    {
        return bae_plugin::native::runner_main();
    }
    startup::run(app_builder().build())
}

/// The AE scripting prelude's bytecode, otherwise compiled by the first script
/// or plugin run. `bae-script` stays free of GPU types, so the task lives here.
struct ScriptPrelude;

impl bae_gpu::WarmUp for ScriptPrelude {
    fn name(&self) -> &str {
        "script prelude"
    }

    fn run(&self, _gpu: &std::sync::Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        bae_script::prewarm()
    }
}

/// Every module of the app, in order. Tests assemble the same app from it (so they
/// see the same commands, panels and translation tables).
fn app_builder() -> AppBuilder {
    let mut builder = AppBuilder::new();
    builder
        .add_module(bae_platform::PlatformModule)
        .add_module(bae_gpu::GpuModule)
        .add_module(model_cli::ModelCliModule)
        .add_module(mcp_cli::McpCliModule)
        // `bae pack`: one self-contained .baez (bae-package).
        .add_module(pack_cli::PackCliModule)
        .add_module(bae_render::RenderModule)
        // Built-in effects (07) in the effect registries renderers and the UI read.
        .add_module(bae_effects::EffectsModule)
        // Motion commands (comp.create, layer.create, key.ease …) for menus and agents.
        .add_module(bae_motion::MotionModule)
        // `script.run`: AE-style scripts (File > Scripts, `bae --headless`, MCP).
        // Plugin host: `bae plugin check|list`; plugin effects in the app registries.
        .add_module(bae_plugin::PluginModule)
        .add_module(shell::ShellModule)
        // Motion workspace panels (project, composition, timeline …).
        .add_module(bae_ui_motion::MotionUiModule);
    // Layer sources drawn by other workstreams (05: shape and text layers), used by the
    // renderer and `bae render`.
    if let Err(error) = builder.register_arc::<dyn bae_render::SourceProvider>(
        bae_vector::SHAPE_PROVIDER,
        std::sync::Arc::new(bae_vector::render::ShapeProvider::new()),
    ) {
        tracing::warn!("shape layer source not registered: {error}");
    }
    let text = bae_text::render::TextProvider::shared();
    // Expressions' `sourceRectAtTime` measures text and shapes with it.
    text.measure_expressions();
    if let Err(error) = builder
        .register_arc::<dyn bae_render::SourceProvider>(bae_text::TEXT_PROVIDER, text.clone())
    {
        tracing::warn!("text layer source not registered: {error}");
    }
    // First-use work done in the background at startup (shell `warm_up`).
    for (id, task) in [
        (
            "bae.vector.renderer",
            std::sync::Arc::new(bae_vector::render::VectorWarmUp)
                as std::sync::Arc<dyn bae_gpu::WarmUp>,
        ),
        ("bae.text.fonts", text),
        ("bae.script.prelude", std::sync::Arc::new(ScriptPrelude)),
    ] {
        if let Err(error) = builder.register_arc::<dyn bae_gpu::WarmUp>(id, task) {
            tracing::warn!("warm-up task {id} not registered: {error}");
        }
    }
    // Feature modules from other workstreams are added here.
    builder
}
