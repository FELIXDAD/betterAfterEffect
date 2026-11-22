//! BAE developer tasks, run as `cargo xtask <task>` (alias in `.cargo/config.toml`).
//!
//! - `size`:  release build of the app, size report and budget check
//!   (`tools/size-budget/budget.toml`, ARCHITECTURE §11).
//! - `dist`:  release build and packaging (`BAE.app` + zip on macOS, zip on Windows).
//! - `bench`: workspace benchmarks (entry point for workstream 04).
//! - `icons`: render the application icon into `assets/icons/app/`.
//!
//! Kept deliberately small: no CLI framework, arguments are parsed by hand.

mod bench;
mod dist;
mod icon;
mod size;
mod util;

use std::process::ExitCode;

const USAGE: &str = "\
BAE developer tasks

Usage: cargo xtask <task> [options]

Tasks:
  size    Build the release app and check it against tools/size-budget/budget.toml
            --target <triple>   build/check for this target (default: host)
            --no-build          measure the existing release binary, do not build
            --json <path>       also write a machine-readable report
            --breakdown         require a per-crate breakdown (needs cargo-bloat)
            --no-breakdown      skip the per-crate breakdown
  dist    Build the release app and package it into <target-dir>/dist/
            --target <triple>   package for this target (default: host)
            --no-build          package the existing release binary, do not build
  bench   Run the workspace benchmarks (criterion)
            cargo xtask bench [<cargo bench options>] [-- <criterion args>]
  icons   Render the application icon into assets/icons/app/ (icns, ico, res, svg)
  help    Show this message

Sizes are reported in MiB (1 MiB = 1024 * 1024 bytes). When GITHUB_STEP_SUMMARY is set,
reports are also appended to the GitHub Actions job summary.
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let rest: Vec<String> = args.collect();
    let result = match task.as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some("size") => size::run(rest),
        Some("dist") => dist::run(rest),
        Some("bench") => bench::run(rest),
        Some("icons") => icon::run(rest),
        Some(other) => Err(anyhow::anyhow!(
            "unknown task `{other}` (run `cargo xtask help`)"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
