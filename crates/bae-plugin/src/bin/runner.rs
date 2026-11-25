//! Runner process for out-of-process native plugins (spec §15): serves one
//! plugin library over stdin/stdout. The application uses `bae plugin runner`
//! (same code); this binary is for tests and tools (`BAE_PLUGIN_RUNNER`).

fn main() -> std::process::ExitCode {
    bae_plugin::native::runner_main()
}
