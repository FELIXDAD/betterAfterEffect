//! `cargo xtask size`: release build of the app, size report and budget check.
//!
//! The budget lives in `tools/size-budget/budget.toml` (ARCHITECTURE §11). The verdict only
//! depends on the stripped release binary; the optional cargo-bloat breakdown is advisory.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::util::{
    self, APP_BIN, APP_PACKAGE, BuildArgs, SizeRow, Target, Verdict, Workspace, human_size,
};

/// Budget file, relative to the workspace root.
pub const BUDGET_FILE: &str = "tools/size-budget/budget.toml";
/// Profile defined in the root Cargo.toml: release with symbols kept, for cargo-bloat.
const ANALYSIS_PROFILE: &str = "size-analysis";
const BLOAT_CRATES: &str = "25";

const USAGE: &str = "\
Usage: cargo xtask size [--target <triple>] [--no-build] [--json <path>] [--breakdown | --no-breakdown]

Builds `bae` in release mode (`cargo build --release -p bae --locked`), reports its size and
fails when it is over the `app` budget in tools/size-budget/budget.toml.
The per-crate breakdown runs `cargo bloat` when it is installed (cargo install cargo-bloat).
";

// ---------------------------------------------------------------------------
// Budget file
// ---------------------------------------------------------------------------

/// One measured artifact in `budget.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub description: String,
    /// Default ceiling in MiB.
    pub max_mib: f64,
    /// Over budget fails the task only when enforced.
    #[serde(default)]
    pub enforced: bool,
    /// Per target triple overrides of `max_mib`.
    #[serde(default)]
    pub targets: BTreeMap<String, TargetBudget>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetBudget {
    pub max_mib: f64,
}

impl Budget {
    /// The ceiling for `triple`: its override if any, otherwise the default.
    pub fn max_mib_for(&self, triple: &str) -> f64 {
        self.targets.get(triple).map_or(self.max_mib, |t| t.max_mib)
    }

    pub fn row(&self, label: String, bytes: u64, triple: &str) -> SizeRow {
        SizeRow {
            label,
            bytes,
            budget_mib: Some(self.max_mib_for(triple)),
            enforced: self.enforced,
        }
    }
}

/// All entries of `budget.toml`, keyed by artifact name (`app`, `app-bundle`, ...).
#[derive(Debug)]
pub struct Budgets(BTreeMap<String, Budget>);

impl Budgets {
    pub fn parse(text: &str) -> Result<Self> {
        let entries: BTreeMap<String, Budget> = toml::from_str(text)?;
        for (name, budget) in &entries {
            check_mib(name, budget.max_mib)?;
            for (triple, over) in &budget.targets {
                check_mib(&format!("{name}.targets.\"{triple}\""), over.max_mib)?;
            }
        }
        Ok(Self(entries))
    }

    pub fn load(ws: &Workspace) -> Result<Self> {
        let path = ws.workspace_root.join(BUDGET_FILE);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid budget file {}", path.display()))
    }

    pub fn get(&self, name: &str) -> Result<&Budget> {
        self.0
            .get(name)
            .with_context(|| format!("{BUDGET_FILE} has no `[{name}]` entry"))
    }
}

fn check_mib(what: &str, mib: f64) -> Result<()> {
    ensure!(
        mib.is_finite() && mib > 0.0 && mib < 1024.0 * 1024.0,
        "`{what}.max_mib` must be a positive number of MiB, got {mib}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Breakdown {
    /// Run cargo-bloat when installed, otherwise print a hint.
    #[default]
    Auto,
    /// `--breakdown`: a missing cargo-bloat is an error.
    Required,
    /// `--no-breakdown`.
    Off,
}

#[derive(Debug, Default)]
struct Args {
    build: BuildArgs,
    json: Option<PathBuf>,
    breakdown: Breakdown,
    help: bool,
}

fn parse_args(args: Vec<String>) -> Result<Args> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if out.build.accept(&arg, &mut it)? {
            continue;
        }
        if let Some(path) = util::option_value(&arg, "--json", &mut it)? {
            out.json = Some(PathBuf::from(path));
            continue;
        }
        match arg.as_str() {
            "--breakdown" => out.breakdown = Breakdown::Required,
            "--no-breakdown" => out.breakdown = Breakdown::Off,
            "-h" | "--help" => out.help = true,
            other => bail!("unknown option `{other}` for `size`\n\n{USAGE}"),
        }
    }
    Ok(out)
}

/// Machine-readable report written by `--json` (and uploaded by CI).
#[derive(Debug, Serialize)]
struct Report<'a> {
    schema: u32,
    artifact: &'a str,
    package: &'a str,
    version: &'a str,
    target: &'a str,
    profile: &'a str,
    binary: String,
    bytes: u64,
    mib: f64,
    budget_mib: f64,
    budget_bytes: u64,
    headroom_bytes: i128,
    enforced: bool,
    within_budget: bool,
    pass: bool,
}

pub fn run(args: Vec<String>) -> Result<()> {
    let args = parse_args(args)?;
    if args.help {
        print!("{USAGE}");
        return Ok(());
    }

    let ws = Workspace::load()?;
    // Fail fast on a broken budget file or a missing required tool, before a long build.
    let budgets = Budgets::load(&ws)?;
    let budget = budgets.get("app")?;
    let bloat_installed = args.breakdown != Breakdown::Off && cargo_bloat_installed();
    if args.breakdown == Breakdown::Required && !bloat_installed {
        bail!("--breakdown needs cargo-bloat: cargo install cargo-bloat");
    }
    let target = Target::resolve(args.build.target)?;
    let version = ws.package(APP_PACKAGE)?.version.as_str();

    if !args.build.no_build {
        ws.build_release_app(&target)?;
    }
    let binary = ws.existing_release_binary(&target)?;
    let bytes = fs::metadata(&binary)
        .with_context(|| format!("failed to stat {}", binary.display()))?
        .len();

    let row = budget.row(
        format!("`{}` (app)", target.exe_name(APP_BIN)),
        bytes,
        &target.triple,
    );
    let verdict = row.verdict();
    let mut markdown = util::size_table(
        &format!("BAE release size: {}", target.triple),
        std::slice::from_ref(&row),
    );
    let _ = writeln!(
        markdown,
        "\n`{APP_PACKAGE}` {version}, profile `release`. Budget `[app]` ({}, {}) from `{BUDGET_FILE}`.",
        budget.description,
        if budget.enforced {
            "enforced"
        } else {
            "not enforced"
        }
    );
    println!("\n{markdown}");
    util::append_step_summary(&markdown)?;

    if let Some(path) = &args.json {
        let report = Report {
            schema: 1,
            artifact: "app",
            package: APP_PACKAGE,
            version,
            target: &target.triple,
            profile: "release",
            binary: binary.display().to_string(),
            bytes,
            mib: util::bytes_to_mib(bytes),
            budget_mib: row.budget_mib.unwrap_or_default(),
            budget_bytes: row.budget_bytes().unwrap_or_default(),
            headroom_bytes: row.headroom_bytes().unwrap_or_default(),
            enforced: row.enforced,
            within_budget: row.headroom_bytes().is_some_and(|h| h >= 0),
            pass: verdict != Verdict::Fail,
        };
        let json = serde_json::to_string_pretty(&report)?;
        fs::write(path, json + "\n")
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!("size report written to {}", path.display());
    }

    // Advisory only: whatever happens here, the verdict above stands.
    match args.breakdown {
        Breakdown::Off => {}
        _ if !bloat_installed => println!(
            "Per-crate breakdown skipped: cargo-bloat is not installed (cargo install cargo-bloat)."
        ),
        _ => breakdown(&ws, &target),
    }

    if verdict == Verdict::Fail {
        bail!(
            "`{}` is {} over its {:.2} MiB budget ({BUDGET_FILE})",
            target.exe_name(APP_BIN),
            human_size(bytes.saturating_sub(row.budget_bytes().unwrap_or_default())),
            row.budget_mib.unwrap_or_default(),
        );
    }
    Ok(())
}

fn cargo_bloat_installed() -> bool {
    util::cargo()
        .args(["bloat", "--version"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Prints the cargo-bloat per-crate table. Advisory: failures only produce a warning.
fn breakdown(ws: &Workspace, target: &Target) {
    let mut cmd = ws.cargo();
    cmd.args([
        "bloat",
        "--profile",
        ANALYSIS_PROFILE,
        "-p",
        APP_PACKAGE,
        "--crates",
        "-n",
        BLOAT_CRATES,
    ]);
    target.add_arg(&mut cmd);
    let table = match util::capture(&mut cmd) {
        Ok(table) => table,
        Err(err) => {
            eprintln!("warning: per-crate breakdown failed (verdict unaffected): {err:#}");
            return;
        }
    };
    let title = format!(
        "Per-crate breakdown (cargo bloat, `{ANALYSIS_PROFILE}` profile: unstripped, sizes are indicative)"
    );
    println!("{title}\n\n{table}");
    let details =
        format!("<details><summary>{title}</summary>\n\n```text\n{table}\n```\n\n</details>\n");
    if let Err(err) = util::append_step_summary(&details) {
        eprintln!("warning: {err:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO_BUDGET: &str = include_str!("../../tools/size-budget/budget.toml");

    #[test]
    fn repository_budget_file_is_valid() {
        let budgets = Budgets::parse(REPO_BUDGET).unwrap();
        let app = budgets.get("app").unwrap();
        assert!(
            app.enforced,
            "the app budget must be enforced (ARCHITECTURE §11)"
        );
        // Invariant instead of a hard-coded number, so budget PRs only touch
        // budget.toml: the executable fits inside the unpacked app bundle.
        let bundle = budgets.get("app-bundle").unwrap();
        for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let exe = app.max_mib_for(target);
            assert!(exe > 0.0, "{target}: app budget must be positive");
            assert!(
                exe <= bundle.max_mib_for(target),
                "{target}: executable budget {exe} exceeds the app bundle budget"
            );
        }
        for name in ["installer", "bae-hub"] {
            budgets.get(name).unwrap();
        }
    }

    #[test]
    fn target_overrides_take_precedence() {
        let budgets = Budgets::parse(
            r#"
            [app]
            description = "bae"
            max_mib = 15.0
            enforced = true
            [app.targets."x86_64-pc-windows-msvc"]
            max_mib = 12.5
            "#,
        )
        .unwrap();
        let app = budgets.get("app").unwrap();
        assert!((app.max_mib_for("x86_64-pc-windows-msvc") - 12.5).abs() < f64::EPSILON);
        assert!((app.max_mib_for("aarch64-apple-darwin") - 15.0).abs() < f64::EPSILON);
        let row = app.row("bae".into(), 100, "x86_64-pc-windows-msvc");
        assert_eq!(row.budget_mib, Some(12.5));
        assert!(row.enforced);
    }

    #[test]
    fn enforced_defaults_to_false() {
        let budgets = Budgets::parse("[hub]\ndescription = \"x\"\nmax_mib = 1\n").unwrap();
        assert!(!budgets.get("hub").unwrap().enforced);
        assert!(budgets.get("app").is_err());
    }

    #[test]
    fn invalid_budgets_are_rejected() {
        // Typo in a field name.
        assert!(Budgets::parse("[app]\ndescription = \"x\"\nmax_mb = 15.0\n").is_err());
        // Missing ceiling.
        assert!(Budgets::parse("[app]\ndescription = \"x\"\n").is_err());
        // Non-positive ceilings, also in overrides.
        assert!(Budgets::parse("[app]\ndescription = \"x\"\nmax_mib = 0.0\n").is_err());
        assert!(
            Budgets::parse(
                "[app]\ndescription = \"x\"\nmax_mib = 1.0\n[app.targets.\"t\"]\nmax_mib = -1.0\n"
            )
            .is_err()
        );
    }

    #[test]
    fn arguments_are_parsed() {
        let args = parse_args(
            [
                "--target",
                "x86_64-pc-windows-msvc",
                "--no-build",
                "--json=r.json",
                "--no-breakdown",
            ]
            .map(String::from)
            .to_vec(),
        )
        .unwrap();
        assert_eq!(args.build.target.as_deref(), Some("x86_64-pc-windows-msvc"));
        assert!(args.build.no_build);
        assert_eq!(args.json, Some(PathBuf::from("r.json")));
        assert_eq!(args.breakdown, Breakdown::Off);

        let args = parse_args(vec!["--breakdown".into()]).unwrap();
        assert_eq!(args.breakdown, Breakdown::Required);
        assert!(args.build.target.is_none() && !args.build.no_build);

        assert!(parse_args(vec!["--bogus".into()]).is_err());
        assert!(parse_args(vec!["--json".into()]).is_err());
    }
}
