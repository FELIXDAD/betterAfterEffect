//! Bundled ScriptUI commands use the installed plugin host and its undo transaction.
use bae_app::AppBuilder;
use bae_plugin::{MemorySource, PluginRuntime, Scope};
use std::sync::Arc;

pub(super) fn register(app: &mut AppBuilder) {
    if let Some(runtime) = app.resource::<PluginRuntime>().cloned() {
        let _ = app.register_arc::<dyn bae_gpu::WarmUp>(
            "bae.script.builtin_panels",
            Arc::new(BuiltinPanels(runtime)),
        );
    }
}

struct BuiltinPanels(PluginRuntime);

impl bae_gpu::WarmUp for BuiltinPanels {
    fn name(&self) -> &str {
        "built-in script panels"
    }
    fn priority(&self) -> u8 {
        1
    }
    fn run(&self, _gpu: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        load(&self.0)
    }
}

pub(crate) fn load(runtime: &PluginRuntime) -> Result<(), String> {
    macro_rules! package {
        ($dir:literal) => {
            MemorySource::new(concat!("builtin:", $dir))
                .with_file(
                    "bae-plugin.toml",
                    include_str!(concat!(
                        "../../../../plugins/builtin/",
                        $dir,
                        "/bae-plugin.toml"
                    )),
                )
                .with_file(
                    "args.json",
                    include_str!(concat!("../../../../plugins/builtin/", $dir, "/args.json")),
                )
                .with_file(
                    "main.js",
                    include_str!(concat!("../../../../plugins/builtin/", $dir, "/main.js")),
                )
        };
    }
    runtime
        .load(&package!("path-nulls"), Scope::Bundled)
        .map_err(|e| e.to_string())?;
    Ok(())
}
