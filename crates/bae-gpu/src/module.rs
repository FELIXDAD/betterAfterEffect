//! [`GpuModule`]: the GPU layer's app module.

use std::process::ExitCode;
use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};
use bae_platform::cli::{self, CliSubcommand};

use crate::caps::GpuReport;
use crate::context::{GpuContext, GpuContextDescriptor};
use crate::viewport::ViewportTextureRegistry;

/// Registers the GPU layer:
///
/// * resource [`GpuContextDescriptor`] (defaults + `BAE_GPU_*` environment; other
///   modules may adjust it in `build` before the context is created),
/// * resource `Arc<ViewportTextureRegistry>`,
/// * the `bae gpu-info [--json]` subcommand.
///
/// The [`GpuContext`] itself is created by the shell (windowed: it needs the
/// window for surface compatibility) or by the headless runner, and inserted as
/// an `Arc<GpuContext>` resource.
#[derive(Debug, Default)]
pub struct GpuModule;

impl AppModule for GpuModule {
    fn name(&self) -> &str {
        "bae-gpu"
    }

    fn build(&self, app: &mut AppBuilder) {
        // The placeholder (with the `--json` argument) must exist first.
        app.add_module(bae_platform::PlatformModule);
        if app.resource::<GpuContextDescriptor>().is_none() {
            app.insert_resource(GpuContextDescriptor::default().with_env());
        }
        if app.resource::<Arc<ViewportTextureRegistry>>().is_none() {
            app.insert_resource(Arc::new(ViewportTextureRegistry::new()));
        }
        let registry = app.registry::<CliSubcommand>();
        let gpu_info = CliSubcommand::new("gpu-info", "").with_run(|ctx| {
            let desc = ctx
                .app
                .resource::<GpuContextDescriptor>()
                .cloned()
                .unwrap_or_default();
            let json = ctx.matches.get_flag("json");
            match GpuContext::new_headless(desc) {
                Ok(gpu) => {
                    let report = GpuReport::new(&gpu);
                    if json {
                        println!("{}", report.to_json());
                    } else {
                        println!("{report}");
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Err(error) => {
                    if json {
                        println!("{}", serde_json::json!({ "error": error.to_string() }));
                    }
                    Err(error.into())
                }
            }
        });
        if let Err(error) = cli::implement(&registry, app.current_owner(), gpu_info) {
            tracing::warn!("gpu-info not registered: {error}");
        }
    }
}
