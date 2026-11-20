//! Shared helpers: cargo invocation, workspace metadata, target selection, size formatting
//! and Markdown reports.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

/// Package name of the desktop application.
pub const APP_PACKAGE: &str = "bae";
/// Executable name of the desktop application (without `.exe`).
pub const APP_BIN: &str = "bae";

const MIB: f64 = 1024.0 * 1024.0;

// ---------------------------------------------------------------------------
// Running cargo and other tools
// ---------------------------------------------------------------------------

/// The cargo that runs xtask (`CARGO` is set by `cargo run`), falling back to `cargo` on PATH.
pub fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")))
}

/// Runs `cmd` with inherited stdio and fails if it does not exit successfully.
pub fn run(cmd: &mut Command) -> Result<()> {
    let line = display_command(cmd);
    eprintln!("$ {line}");
    let status = cmd
        .status()
        .with_context(|| format!("failed to start `{line}`"))?;
    if !status.success() {
        bail!("`{line}` failed ({status})");
    }
    Ok(())
}

/// Runs `cmd` and returns its stdout; stderr is inherited so progress stays visible.
pub fn capture(cmd: &mut Command) -> Result<String> {
    let line = display_command(cmd);
    let output = cmd
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to start `{line}`"))?;
    if !output.status.success() {
        bail!("`{line}` failed ({})", output.status);
    }
    String::from_utf8(output.stdout).with_context(|| format!("`{line}` printed non-UTF-8 output"))
}

fn display_command(cmd: &Command) -> String {
    let program = Path::new(cmd.get_program()).file_stem().map_or_else(
        || cmd.get_program().to_string_lossy(),
        |s| s.to_string_lossy(),
    );
    let mut line = program.into_owned();
    for arg in cmd.get_args() {
        line.push(' ');
        line.push_str(&arg.to_string_lossy());
    }
    line
}

// ---------------------------------------------------------------------------
// Workspace metadata
// ---------------------------------------------------------------------------

/// The parts of `cargo metadata --no-deps` xtask needs.
#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub workspace_root: PathBuf,
    /// Honours `CARGO_TARGET_DIR` and `build.target-dir`; never assume `./target`.
    pub target_directory: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
pub struct CargoTarget {
    pub kind: Vec<String>,
}

impl Workspace {
    pub fn load() -> Result<Self> {
        // Start from xtask's own directory so `cargo xtask` works from any cwd. Prefer the
        // runtime value set by `cargo run`: with a target dir shared between checkouts, the
        // binary may have been compiled from another checkout.
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|| OsString::from(env!("CARGO_MANIFEST_DIR")));
        let json = capture(
            cargo()
                .args(["metadata", "--format-version", "1", "--no-deps"])
                .current_dir(manifest_dir),
        )?;
        serde_json::from_str(&json).context("failed to parse `cargo metadata` output")
    }

    pub fn package(&self, name: &str) -> Result<&Package> {
        self.packages
            .iter()
            .find(|p| p.name == name)
            .with_context(|| format!("package `{name}` is not a workspace member"))
    }

    /// A cargo command that runs in the workspace root.
    pub fn cargo(&self) -> Command {
        let mut cmd = cargo();
        cmd.current_dir(&self.workspace_root);
        cmd
    }

    /// `cargo build --release -p bae --locked [--target <triple>]`.
    pub fn build_release_app(&self, target: &Target) -> Result<()> {
        let mut cmd = self.cargo();
        cmd.args(["build", "--release", "--locked", "-p", APP_PACKAGE]);
        target.add_arg(&mut cmd);
        run(&mut cmd)
    }

    /// `<target-dir>[/<triple>]/release/bae[.exe]`.
    pub fn release_binary(&self, target: &Target) -> PathBuf {
        let mut dir = self.target_directory.clone();
        if target.explicit {
            dir.push(&target.triple);
        }
        dir.join("release").join(target.exe_name(APP_BIN))
    }

    /// Fails with a hint when a `--no-build` run finds no binary.
    pub fn existing_release_binary(&self, target: &Target) -> Result<PathBuf> {
        let path = self.release_binary(target);
        if !path.is_file() {
            bail!(
                "release binary not found at {} (run without --no-build first)",
                path.display()
            );
        }
        Ok(path)
    }
}

// ---------------------------------------------------------------------------
// Targets
// ---------------------------------------------------------------------------

/// The target triple being built. `explicit` means `--target` is passed to cargo, which
/// moves the output into `<target-dir>/<triple>/`.
#[derive(Debug, Clone)]
pub struct Target {
    pub triple: String,
    pub explicit: bool,
}

impl Target {
    pub fn resolve(requested: Option<String>) -> Result<Self> {
        Ok(match requested {
            Some(triple) => Self {
                triple,
                explicit: true,
            },
            None => Self {
                triple: host_triple()?,
                explicit: false,
            },
        })
    }

    pub fn add_arg(&self, cmd: &mut Command) {
        if self.explicit {
            cmd.args(["--target", &self.triple]);
        }
    }

    pub fn is_windows(&self) -> bool {
        self.triple.contains("-windows")
    }

    pub fn is_macos(&self) -> bool {
        self.triple.contains("-apple-darwin")
    }

    /// First component of the triple (`aarch64`, `x86_64`).
    pub fn arch(&self) -> &str {
        self.triple.split('-').next().unwrap_or(&self.triple)
    }

    pub fn exe_name(&self, name: &str) -> String {
        if self.is_windows() {
            format!("{name}.exe")
        } else {
            name.to_owned()
        }
    }
}

/// The host triple from `rustc -vV` (honours rust-toolchain.toml through rustup).
pub fn host_triple() -> Result<String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let out = capture(Command::new(rustc).arg("-vV"))?;
    parse_host_triple(&out).context("`rustc -vV` printed no `host:` line")
}

fn parse_host_triple(rustc_vv: &str) -> Option<String> {
    rustc_vv
        .lines()
        .find_map(|line| line.strip_prefix("host:"))
        .map(|host| host.trim().to_owned())
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

/// Options shared by `size` and `dist`.
#[derive(Debug, Default)]
pub struct BuildArgs {
    pub target: Option<String>,
    pub no_build: bool,
}

impl BuildArgs {
    /// Consumes `arg` if it is a shared option; returns `false` for anything else.
    pub fn accept(&mut self, arg: &str, rest: &mut impl Iterator<Item = String>) -> Result<bool> {
        if arg == "--no-build" {
            self.no_build = true;
        } else if let Some(value) = option_value(arg, "--target", rest)? {
            self.target = Some(value);
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}

/// Parses `--name value` and `--name=value`; `None` if `arg` is not `--name`.
pub fn option_value(
    arg: &str,
    name: &str,
    rest: &mut impl Iterator<Item = String>,
) -> Result<Option<String>> {
    if arg == name {
        let value = rest
            .next()
            .with_context(|| format!("`{name}` needs a value"))?;
        return Ok(Some(value));
    }
    Ok(arg
        .strip_prefix(name)
        .and_then(|tail| tail.strip_prefix('='))
        .map(str::to_owned))
}

// ---------------------------------------------------------------------------
// Sizes
// ---------------------------------------------------------------------------

pub fn mib_to_bytes(mib: f64) -> u64 {
    // Budgets are small positive numbers (validated on load), so the cast cannot overflow.
    (mib * MIB).round() as u64
}

pub fn bytes_to_mib(bytes: u64) -> f64 {
    bytes as f64 / MIB
}

/// `12939264` -> `12,939,264`.
pub fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Binary-prefixed human size: `512 B`, `1.50 KiB`, `12.34 MiB`, `1.02 GiB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in UNITS {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.2} {unit}")
}

/// Recursive sum of file sizes (symlinks are not followed).
pub fn dir_size(path: &Path) -> Result<u64> {
    let meta =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if !meta.is_dir() {
        return Ok(meta.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        total += dir_size(&entry?.path())?;
    }
    Ok(total)
}

// ---------------------------------------------------------------------------
// Size reports
// ---------------------------------------------------------------------------

/// One measured artifact and its budget, if it has one.
#[derive(Debug, Clone)]
pub struct SizeRow {
    pub label: String,
    pub bytes: u64,
    pub budget_mib: Option<f64>,
    /// Over-budget fails the task only when the budget is enforced.
    pub enforced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// Budget exists but is not enforced yet; `over` tells whether it would fail.
    Info {
        over: bool,
    },
    NoBudget,
}

impl SizeRow {
    pub fn budget_bytes(&self) -> Option<u64> {
        self.budget_mib.map(mib_to_bytes)
    }

    /// Budget minus size; negative when over budget.
    pub fn headroom_bytes(&self) -> Option<i128> {
        self.budget_bytes()
            .map(|budget| i128::from(budget) - i128::from(self.bytes))
    }

    pub fn verdict(&self) -> Verdict {
        match (self.headroom_bytes(), self.enforced) {
            (None, _) => Verdict::NoBudget,
            (Some(h), true) if h >= 0 => Verdict::Pass,
            (Some(_), true) => Verdict::Fail,
            (Some(h), false) => Verdict::Info { over: h < 0 },
        }
    }
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL (over budget)",
            Self::Info { over: false } => "within (not enforced)",
            Self::Info { over: true } => "over (not enforced)",
            Self::NoBudget => "-",
        }
    }
}

/// A Markdown section with one table row per artifact.
pub fn size_table(title: &str, rows: &[SizeRow]) -> String {
    let mut md = format!("### {title}\n\n");
    md.push_str("| Artifact | Size | Budget | Headroom | Result |\n");
    md.push_str("|---|---:|---:|---:|---|\n");
    for row in rows {
        let budget = row
            .budget_mib
            .map_or_else(|| "-".to_owned(), |mib| format!("{mib:.2} MiB"));
        let headroom = match (row.headroom_bytes(), row.budget_bytes()) {
            (Some(h), Some(budget)) => {
                let sign = if h < 0 { "-" } else { "+" };
                let abs = u64::try_from(h.unsigned_abs()).unwrap_or(u64::MAX);
                let pct = h as f64 / budget.max(1) as f64 * 100.0;
                format!("{sign}{:.2} MiB ({pct:.1}%)", bytes_to_mib(abs))
            }
            _ => "-".to_owned(),
        };
        let _ = writeln!(
            md,
            "| {} | {:.2} MiB ({} B) | {budget} | {headroom} | {} |",
            row.label,
            bytes_to_mib(row.bytes),
            group_digits(row.bytes),
            row.verdict().label(),
        );
    }
    md
}

/// Appends Markdown to the GitHub Actions job summary when running in CI.
pub fn append_step_summary(markdown: &str) -> Result<()> {
    let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open GITHUB_STEP_SUMMARY ({})", path.display()))?;
    writeln!(file, "{markdown}").context("failed to write GITHUB_STEP_SUMMARY")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_grouped() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(12_939_264), "12,939,264");
        assert_eq!(group_digits(100_000), "100,000");
    }

    #[test]
    fn human_sizes_use_binary_prefixes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.50 KiB");
        assert_eq!(human_size(15 * 1024 * 1024), "15.00 MiB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024 / 2), "1.50 GiB");
    }

    #[test]
    fn mib_conversions_round_trip() {
        assert_eq!(mib_to_bytes(15.0), 15_728_640);
        assert_eq!(mib_to_bytes(0.5), 524_288);
        assert!((bytes_to_mib(15_728_640) - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn host_triple_is_parsed_from_rustc_output() {
        let out = "rustc 1.100.0 (0ef1bc386 2026-11-09)\nbinary: rustc\nhost: aarch64-apple-darwin\nrelease: 1.100.0\n";
        assert_eq!(
            parse_host_triple(out).as_deref(),
            Some("aarch64-apple-darwin")
        );
        assert_eq!(parse_host_triple("release: 1.0\n"), None);
    }

    #[test]
    fn target_naming() {
        let win = Target {
            triple: "x86_64-pc-windows-msvc".into(),
            explicit: true,
        };
        assert!(win.is_windows() && !win.is_macos());
        assert_eq!(win.exe_name("bae"), "bae.exe");
        assert_eq!(win.arch(), "x86_64");
        let mac = Target {
            triple: "aarch64-apple-darwin".into(),
            explicit: false,
        };
        assert!(mac.is_macos() && !mac.is_windows());
        assert_eq!(mac.exe_name("bae"), "bae");
        assert_eq!(mac.arch(), "aarch64");
    }

    #[test]
    fn options_accept_both_spellings() {
        let mut rest = vec!["x86_64-pc-windows-msvc".to_owned()].into_iter();
        assert_eq!(
            option_value("--target", "--target", &mut rest).unwrap(),
            Some("x86_64-pc-windows-msvc".to_owned())
        );
        let mut empty = std::iter::empty();
        assert_eq!(
            option_value("--target=a-b-c", "--target", &mut empty).unwrap(),
            Some("a-b-c".to_owned())
        );
        assert_eq!(
            option_value("--targets", "--target", &mut empty).unwrap(),
            None
        );
        assert!(option_value("--target", "--target", &mut empty).is_err());
    }

    #[test]
    fn verdicts_follow_budget_and_enforcement() {
        let mut row = SizeRow {
            label: "bae".into(),
            bytes: mib_to_bytes(10.0),
            budget_mib: Some(15.0),
            enforced: true,
        };
        assert_eq!(row.verdict(), Verdict::Pass);
        assert_eq!(row.headroom_bytes(), Some(i128::from(mib_to_bytes(5.0))));
        row.bytes = mib_to_bytes(16.0);
        assert_eq!(row.verdict(), Verdict::Fail);
        row.enforced = false;
        assert_eq!(row.verdict(), Verdict::Info { over: true });
        row.budget_mib = None;
        assert_eq!(row.verdict(), Verdict::NoBudget);
        // Exactly at the budget passes.
        let at = SizeRow {
            label: "bae".into(),
            bytes: mib_to_bytes(15.0),
            budget_mib: Some(15.0),
            enforced: true,
        };
        assert_eq!(at.verdict(), Verdict::Pass);
    }

    #[test]
    fn size_table_renders_markdown() {
        let rows = [
            SizeRow {
                label: "`bae`".into(),
                bytes: mib_to_bytes(12.0),
                budget_mib: Some(15.0),
                enforced: true,
            },
            SizeRow {
                label: "zip".into(),
                bytes: 1000,
                budget_mib: None,
                enforced: false,
            },
        ];
        let md = size_table("Release size", &rows);
        assert!(md.starts_with("### Release size\n"));
        assert!(md.contains(
            "| `bae` | 12.00 MiB (12,582,912 B) | 15.00 MiB | +3.00 MiB (20.0%) | PASS |"
        ));
        assert!(md.contains("| zip | 0.00 MiB (1,000 B) | - | - | - |"));
        let over = SizeRow {
            label: "bae".into(),
            bytes: mib_to_bytes(16.5),
            budget_mib: Some(15.0),
            enforced: true,
        };
        assert!(size_table("t", &[over]).contains("-1.50 MiB (-10.0%) | FAIL (over budget) |"));
    }
}
