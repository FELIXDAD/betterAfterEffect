//! `cargo xtask dist`: release build and packaging into `<target-dir>/dist/`.
//!
//! - macOS: `BAE.app` (unsigned) and `BAE-<version>-<arch>-macos.zip` of it.
//! - Windows: `BAE-<version>-<arch>-windows/` and a zip of its contents.
//!
//! Sizes are compared with the `app-bundle` and `installer` budgets; they fail the task only
//! once those budgets are marked `enforced` in `tools/size-budget/budget.toml`.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

use crate::size::Budgets;
use crate::util::{self, APP_BIN, APP_PACKAGE, BuildArgs, Target, Verdict, Workspace};

const USAGE: &str = "\
Usage: cargo xtask dist [--target <triple>] [--no-build]

Builds `bae` in release mode and packages it into <target-dir>/dist/:
  macOS    BAE.app (not signed) and BAE-<version>-<arch>-macos.zip
  Windows  BAE-<version>-<arch>-windows.zip (bae.exe, LICENSE, README.md)
";

const BUNDLE_ID: &str = "io.github.felixdad.bae";
const BUNDLE_NAME: &str = "BAE";
const MIN_MACOS: &str = "12.0";

pub fn run(args: Vec<String>) -> Result<()> {
    let mut build = BuildArgs::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if build.accept(&arg, &mut it)? {
            continue;
        }
        if arg == "-h" || arg == "--help" {
            print!("{USAGE}");
            return Ok(());
        }
        bail!("unknown option `{arg}` for `dist`\n\n{USAGE}");
    }

    let ws = Workspace::load()?;
    let budgets = Budgets::load(&ws)?;
    let target = Target::resolve(build.target)?;
    if !target.is_macos() && !target.is_windows() {
        bail!(
            "`dist` packages macOS and Windows targets only, not `{}`",
            target.triple
        );
    }
    let version = ws.package(APP_PACKAGE)?.version.clone();

    if !build.no_build {
        ws.build_release_app(&target)?;
    }
    let binary = ws.existing_release_binary(&target)?;
    let dist = ws.target_directory.join("dist");
    fs::create_dir_all(&dist).with_context(|| format!("failed to create {}", dist.display()))?;
    let stamp = zip_timestamp();

    let (unpacked, archive) = if target.is_macos() {
        package_macos(&ws, &binary, &version, &target, &dist, stamp)?
    } else {
        package_windows(&ws, &binary, &version, &target, &dist, stamp)?
    };

    let unpacked_bytes = util::dir_size(&unpacked)?;
    let archive_bytes = util::dir_size(&archive)?;
    let rows = [
        budgets.get("app-bundle")?.row(
            format!("`{}` (unpacked)", file_name(&unpacked)),
            unpacked_bytes,
            &target.triple,
        ),
        budgets.get("installer")?.row(
            format!("`{}`", file_name(&archive)),
            archive_bytes,
            &target.triple,
        ),
    ];
    println!("\nPackaged {BUNDLE_NAME} {version} for {}:", target.triple);
    println!(
        "  {}  ({})",
        unpacked.display(),
        util::human_size(unpacked_bytes)
    );
    println!(
        "  {}  ({})",
        archive.display(),
        util::human_size(archive_bytes)
    );
    if target.is_macos() {
        println!(
            "  note: BAE.app is not code-signed or notarized; on first launch use right-click > Open."
        );
    }
    let markdown = util::size_table(&format!("BAE package: {}", target.triple), &rows);
    println!("\n{markdown}");
    util::append_step_summary(&markdown)?;

    if rows.iter().any(|row| row.verdict() == Verdict::Fail) {
        bail!(
            "package is over an enforced budget in {}",
            crate::size::BUDGET_FILE
        );
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Builds `dist/BAE.app` and zips it. Returns (bundle dir, zip path).
fn package_macos(
    ws: &Workspace,
    binary: &Path,
    version: &str,
    target: &Target,
    dist: &Path,
    stamp: DateTime,
) -> Result<(PathBuf, PathBuf)> {
    let app = dist.join(format!("{BUNDLE_NAME}.app"));
    recreate_dir(&app)?;
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    for dir in [&macos, &resources] {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    write_file(&contents.join("Info.plist"), info_plist(version).as_bytes())?;
    copy_file(binary, &macos.join(APP_BIN))?;
    copy_file(
        &ws.workspace_root.join("LICENSE"),
        &resources.join("LICENSE"),
    )?;

    let zip = dist.join(format!(
        "{BUNDLE_NAME}-{version}-{}-macos.zip",
        target.arch()
    ));
    zip_dir(&app, &format!("{BUNDLE_NAME}.app"), &zip, stamp)?;
    Ok((app, zip))
}

/// Builds `dist/BAE-<version>-<arch>-windows/` and zips its contents (at the archive root).
fn package_windows(
    ws: &Workspace,
    binary: &Path,
    version: &str,
    target: &Target,
    dist: &Path,
    stamp: DateTime,
) -> Result<(PathBuf, PathBuf)> {
    let name = format!("{BUNDLE_NAME}-{version}-{}-windows", target.arch());
    let stage = dist.join(&name);
    recreate_dir(&stage)?;
    copy_file(binary, &stage.join(target.exe_name(APP_BIN)))?;
    for doc in ["LICENSE", "README.md"] {
        copy_file(&ws.workspace_root.join(doc), &stage.join(doc))?;
    }
    let zip = dist.join(format!("{name}.zip"));
    zip_dir(&stage, "", &zip, stamp)?;
    Ok((stage, zip))
}

fn recreate_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("failed to remove {}", dir.display()))?;
    }
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    fs::copy(from, to)
        .with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Info.plist
// ---------------------------------------------------------------------------

enum PlistValue<'a> {
    String(&'a str),
    Bool(bool),
}

/// The bundle's `Info.plist` (XML property list).
pub fn info_plist(version: &str) -> String {
    use PlistValue::{Bool, String as Str};
    let bundle_version = bundle_version(version);
    let entries = [
        ("CFBundleName", Str(BUNDLE_NAME)),
        ("CFBundleDisplayName", Str(BUNDLE_NAME)),
        ("CFBundleIdentifier", Str(BUNDLE_ID)),
        ("CFBundleExecutable", Str(APP_BIN)),
        ("CFBundlePackageType", Str("APPL")),
        ("CFBundleShortVersionString", Str(bundle_version)),
        ("CFBundleVersion", Str(bundle_version)),
        ("CFBundleInfoDictionaryVersion", Str("6.0")),
        ("LSMinimumSystemVersion", Str(MIN_MACOS)),
        ("NSHighResolutionCapable", Bool(true)),
        ("NSSupportsAutomaticGraphicsSwitching", Bool(true)),
        (
            "LSApplicationCategoryType",
            Str("public.app-category.video"),
        ),
        ("NSHumanReadableCopyright", Str("Apache-2.0")),
    ];
    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    for (key, value) in entries {
        xml.push_str(&format!("\t<key>{}</key>\n", xml_escape(key)));
        match value {
            Str(s) => xml.push_str(&format!("\t<string>{}</string>\n", xml_escape(s))),
            Bool(true) => xml.push_str("\t<true/>\n"),
            Bool(false) => xml.push_str("\t<false/>\n"),
        }
    }
    xml.push_str("</dict>\n</plist>\n");
    xml
}

/// macOS wants up to three dot-separated integers: drop SemVer pre-release/build suffixes.
fn bundle_version(version: &str) -> &str {
    version.split(['-', '+']).next().unwrap_or(version)
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Zip
// ---------------------------------------------------------------------------

/// Zips `src` recursively. Entries are named `<prefix>/<relative path>` (`prefix` may be
/// empty), sorted for reproducibility, with directories stored and unix permissions set:
/// 0o755 for executables (anything in `Contents/MacOS/`, `.exe`, or executable on disk).
fn zip_dir(src: &Path, prefix: &str, zip_path: &Path, stamp: DateTime) -> Result<()> {
    let file = File::create(zip_path)
        .with_context(|| format!("failed to create {}", zip_path.display()))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let base = SimpleFileOptions::default()
        .system(System::Unix)
        .last_modified_time(stamp);
    if !prefix.is_empty() {
        zip.add_directory(prefix, base.unix_permissions(0o755))?;
    }
    let mut entries = Vec::new();
    collect_entries(src, "", &mut entries)?;
    for (rel, path, is_dir) in entries {
        let name = if prefix.is_empty() {
            rel
        } else {
            format!("{prefix}/{rel}")
        };
        if is_dir {
            zip.add_directory(name, base.unix_permissions(0o755))?;
            continue;
        }
        let mode = if is_executable(&name, &path)? {
            0o755
        } else {
            0o644
        };
        zip.start_file(
            name,
            base.compression_method(CompressionMethod::Deflated)
                .compression_level(Some(9))
                .unix_permissions(mode),
        )?;
        let mut input =
            File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        io::copy(&mut input, &mut zip)
            .with_context(|| format!("failed to compress {}", path.display()))?;
    }
    zip.finish()?
        .flush()
        .with_context(|| format!("failed to write {}", zip_path.display()))
}

/// Depth-first, sorted: (relative name with `/`, absolute path, is directory).
fn collect_entries(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf, bool)>) -> Result<()> {
    let mut children = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .collect::<io::Result<Vec<_>>>()?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        let name = child.file_name().to_string_lossy().into_owned();
        let rel = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        let path = child.path();
        if child.file_type()?.is_dir() {
            out.push((rel.clone(), path.clone(), true));
            collect_entries(&path, &rel, out)?;
        } else {
            out.push((rel, path, false));
        }
    }
    Ok(())
}

fn is_executable(name: &str, path: &Path) -> Result<bool> {
    let by_name = name
        .rsplit_once('/')
        .is_some_and(|(dir, _)| dir.ends_with("Contents/MacOS"))
        || name.ends_with(".exe");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(path)?.permissions().mode();
        Ok(by_name || mode & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(by_name)
    }
}

/// Entry timestamp: `SOURCE_DATE_EPOCH` when set (reproducible builds), otherwise now (UTC).
fn zip_timestamp() -> DateTime {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
    dos_datetime(secs)
}

/// Unix seconds to a zip (MS-DOS) timestamp; out-of-range times become 1980-01-01.
fn dos_datetime(unix_secs: u64) -> DateTime {
    let (year, month, day) = civil_from_days(unix_secs / 86_400);
    let secs_of_day = unix_secs % 86_400;
    let (Ok(year), Ok(hour), Ok(minute), Ok(second)) = (
        u16::try_from(year),
        u8::try_from(secs_of_day / 3600),
        u8::try_from(secs_of_day / 60 % 60),
        u8::try_from(secs_of_day % 60),
    ) else {
        return DateTime::default();
    };
    DateTime::from_date_and_time(year, month, day, hour, minute, second).unwrap_or_default()
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian calendar
/// (Howard Hinnant's `civil_from_days`, restricted to non-negative days).
fn civil_from_days(days: u64) -> (u64, u8, u8) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    // month is 1..=12 and day 1..=31 by construction.
    (year, month as u8, day as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_has_every_required_key() {
        let xml = info_plist("0.1.0");
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist"));
        assert!(xml.ends_with("</dict>\n</plist>\n"));
        for (key, value) in [
            ("CFBundleName", "<string>BAE</string>"),
            ("CFBundleDisplayName", "<string>BAE</string>"),
            (
                "CFBundleIdentifier",
                "<string>io.github.felixdad.bae</string>",
            ),
            ("CFBundleExecutable", "<string>bae</string>"),
            ("CFBundlePackageType", "<string>APPL</string>"),
            ("CFBundleShortVersionString", "<string>0.1.0</string>"),
            ("CFBundleVersion", "<string>0.1.0</string>"),
            ("CFBundleInfoDictionaryVersion", "<string>6.0</string>"),
            ("LSMinimumSystemVersion", "<string>12.0</string>"),
            ("NSHighResolutionCapable", "<true/>"),
            ("NSSupportsAutomaticGraphicsSwitching", "<true/>"),
            (
                "LSApplicationCategoryType",
                "<string>public.app-category.video</string>",
            ),
            ("NSHumanReadableCopyright", "<string>Apache-2.0</string>"),
        ] {
            let pair = format!("\t<key>{key}</key>\n\t{value}\n");
            assert!(xml.contains(&pair), "missing {pair:?} in\n{xml}");
        }
        assert_eq!(xml.matches("<key>").count(), 13);
        assert_eq!(
            xml.matches("<key>").count(),
            xml.matches("<string>").count() + xml.matches("<true/>").count()
        );
    }

    #[test]
    fn bundle_version_drops_semver_suffixes() {
        assert_eq!(bundle_version("0.1.0"), "0.1.0");
        assert_eq!(bundle_version("1.2.3-alpha.1"), "1.2.3");
        assert_eq!(bundle_version("1.2.3+build.5"), "1.2.3");
        assert!(info_plist("1.0.0-rc.1").contains("<string>1.0.0</string>"));
    }

    #[test]
    fn xml_is_escaped() {
        assert_eq!(xml_escape("a&b<c>\"d'"), "a&amp;b&lt;c&gt;&quot;d&apos;");
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(1_700_000_000 / 86_400), (2023, 11, 14));
    }

    #[test]
    fn dos_timestamps() {
        let t = dos_datetime(1_700_000_000); // 2023-11-14 22:13:20 UTC
        assert_eq!(
            (
                t.year(),
                t.month(),
                t.day(),
                t.hour(),
                t.minute(),
                t.second()
            ),
            (2023, 11, 14, 22, 13, 20)
        );
        // Before 1980 is not representable: falls back to 1980-01-01.
        let t = dos_datetime(0);
        assert_eq!((t.year(), t.month(), t.day()), (1980, 1, 1));
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("bae-xtask-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A workspace rooted at the real repository (for LICENSE / README.md).
    fn test_workspace(target_directory: &Path) -> Workspace {
        Workspace {
            workspace_root: Path::new(env!("CARGO_MANIFEST_DIR")).join(".."),
            target_directory: target_directory.to_owned(),
            packages: Vec::new(),
        }
    }

    /// (name, is directory, unix mode) of every entry.
    fn zip_entries(path: &Path) -> Vec<(String, bool, u32)> {
        let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| {
                let entry = archive.by_index(i).unwrap();
                (
                    entry.name().unwrap().into_owned(),
                    entry.is_dir(),
                    entry.unix_mode().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn macos_package_layout_and_modes() {
        let tmp = scratch_dir("mac");
        let binary = tmp.join("bae-input");
        fs::write(&binary, b"binary").unwrap();
        let target = Target {
            triple: "aarch64-apple-darwin".into(),
            explicit: false,
        };
        let stamp = dos_datetime(1_700_000_000);
        let (app, zip) = package_macos(
            &test_workspace(&tmp),
            &binary,
            "0.1.0",
            &target,
            &tmp,
            stamp,
        )
        .unwrap();
        assert_eq!(file_name(&app), "BAE.app");
        assert_eq!(file_name(&zip), "BAE-0.1.0-aarch64-macos.zip");
        assert_eq!(
            fs::read_to_string(app.join("Contents/Info.plist")).unwrap(),
            info_plist("0.1.0")
        );
        let entries = zip_entries(&zip);
        fs::remove_dir_all(&tmp).unwrap();

        let dir = |name: &str| (name.to_owned(), true, 0o040_755);
        let file = |name: &str, mode: u32| (name.to_owned(), false, 0o100_000 | mode);
        assert_eq!(
            entries,
            [
                dir("BAE.app/"),
                dir("BAE.app/Contents/"),
                file("BAE.app/Contents/Info.plist", 0o644),
                dir("BAE.app/Contents/MacOS/"),
                file("BAE.app/Contents/MacOS/bae", 0o755),
                dir("BAE.app/Contents/Resources/"),
                file("BAE.app/Contents/Resources/LICENSE", 0o644),
            ]
        );
    }

    #[test]
    fn windows_package_layout() {
        let tmp = scratch_dir("win");
        let binary = tmp.join("bae-input.exe");
        fs::write(&binary, b"MZ").unwrap();
        let target = Target {
            triple: "x86_64-pc-windows-msvc".into(),
            explicit: true,
        };
        let (stage, zip) = package_windows(
            &test_workspace(&tmp),
            &binary,
            "0.1.0",
            &target,
            &tmp,
            DateTime::default(),
        )
        .unwrap();
        assert_eq!(file_name(&stage), "BAE-0.1.0-x86_64-windows");
        assert_eq!(file_name(&zip), "BAE-0.1.0-x86_64-windows.zip");
        let names: Vec<String> = zip_entries(&zip).into_iter().map(|e| e.0).collect();
        fs::remove_dir_all(&tmp).unwrap();
        assert_eq!(names, ["LICENSE", "README.md", "bae.exe"]);
    }
}
