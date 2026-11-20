//! `cargo xtask bench`: entry point for the workspace benchmarks (workstream 04 adds the
//! criterion benches and the B1-B3 scenarios from ARCHITECTURE §11).
//!
//! Without cargo options this runs `cargo bench --workspace --bench '*'`: only `[[bench]]`
//! targets. Plain `cargo bench --workspace` would also build every lib and bin test harness
//! in the (fat-LTO) bench profile and hand criterion arguments to libtest, which rejects them.

use anyhow::Result;

use crate::util::{self, Workspace};

const USAGE: &str = "\
Usage: cargo xtask bench [<cargo bench options>] [-- <criterion args>]

Without cargo options, runs every benchmark target in the workspace:
  cargo bench --workspace --bench '*' [-- <criterion args>]
With options, they replace the defaults, e.g. `cargo xtask bench -p bae-render`.
Examples:
  cargo xtask bench -- --save-baseline main
  cargo xtask bench -- --baseline main
";

pub fn run(args: Vec<String>) -> Result<()> {
    let (cargo_opts, criterion) = split_args(args);
    if cargo_opts.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let ws = Workspace::load()?;
    if cargo_opts.is_empty() {
        let benches = ws
            .packages
            .iter()
            .flat_map(|p| &p.targets)
            .filter(|t| t.kind.iter().any(|k| k == "bench"))
            .count();
        if benches == 0 {
            // `--bench '*'` errors when nothing matches; no benches yet is not a failure.
            println!("No benchmark targets in the workspace yet (workstream 04 adds them).");
            return Ok(());
        }
    }
    let mut cmd = ws.cargo();
    cmd.args(bench_args(cargo_opts, criterion));
    util::run(&mut cmd)
}

/// Splits at the first `--`: (cargo options, arguments for the bench harness).
fn split_args(args: Vec<String>) -> (Vec<String>, Option<Vec<String>>) {
    match args.iter().position(|a| a == "--") {
        Some(i) => {
            let mut cargo_opts = args;
            let rest = cargo_opts.split_off(i + 1);
            cargo_opts.pop();
            (cargo_opts, Some(rest))
        }
        None => (args, None),
    }
}

fn bench_args(cargo_opts: Vec<String>, criterion: Option<Vec<String>>) -> Vec<String> {
    let mut out = vec!["bench".to_owned()];
    if cargo_opts.is_empty() {
        out.extend(["--workspace", "--bench", "*"].map(String::from));
    } else {
        out.extend(cargo_opts);
    }
    if let Some(rest) = criterion {
        out.push("--".to_owned());
        out.extend(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(s: &[&str]) -> Vec<String> {
        s.iter().map(|&s| s.to_owned()).collect()
    }

    #[test]
    fn defaults_select_every_bench_target() {
        let (opts, rest) = split_args(vec![]);
        assert_eq!(
            bench_args(opts, rest),
            strings(&["bench", "--workspace", "--bench", "*"])
        );
    }

    #[test]
    fn criterion_args_are_forwarded() {
        let (opts, rest) = split_args(strings(&["--", "--save-baseline", "main", "--", "x"]));
        assert!(opts.is_empty());
        assert_eq!(
            bench_args(opts, rest),
            strings(&[
                "bench",
                "--workspace",
                "--bench",
                "*",
                "--",
                "--save-baseline",
                "main",
                "--",
                "x"
            ])
        );
    }

    #[test]
    fn cargo_options_replace_defaults() {
        let (opts, rest) = split_args(strings(&["-p", "bae-render", "--", "--quick"]));
        assert_eq!(
            bench_args(opts, rest),
            strings(&["bench", "-p", "bae-render", "--", "--quick"])
        );
    }
}
