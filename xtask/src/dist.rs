//! `cargo xtask dist`: release build and packaging into `<target-dir>/dist/`.
//!
//! - macOS: `BAE.app` (ad-hoc signed, not notarized) and `BAE-<version>-<arch>-macos.zip` of it.
//!   The bundle has the app icon (`assets/icons/app/bae.icns`) and declares the `.bae`
//!   (project folder, shown as one document) and `.baez` (packed project) document types.
//! - Windows: `BAE-<version>-<arch>-windows/` and a zip of its contents. `bae.exe` must carry
//!   the app icon (linked in by `apps/bae/build.rs`); packaging fails when it does not.
//!
//! Sizes are compared with the `app-bundle` and `installer` budgets; they fail the task only
//! once those budgets are marked `enforced` in `tools/size-budget/budget.toml`.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

use crate::icon;
use crate::size::Budgets;
use crate::util::{self, APP_BIN, APP_PACKAGE, BuildArgs, Target, Verdict, Workspace};

const USAGE: &str = "\
Usage: cargo xtask dist [--target <triple>] [--no-build]

Builds `bae` in release mode and packages it into <target-dir>/dist/:
  macOS    BAE.app (ad-hoc signed, not notarized) and BAE-<version>-<arch>-macos.zip
  Windows  BAE-<version>-<arch>-windows.zip (bae.exe, LICENSE, README.md)
Both include ThirdPartyNotices/ (licenses of bundled third-party assets).
";

/// Licenses and notices of third-party assets compiled into `bae` (source in the
/// repository, path inside `ThirdPartyNotices/`): Adobe's open-source Spectrum icons
/// (Apache-2.0 section 4: ship the license and the copyright notice).
const THIRD_PARTY_NOTICES: &[(&str, &str)] = &[
    ("assets/icons/spectrum/LICENSE", "Spectrum-Icons/LICENSE"),
    (
        "assets/icons/spectrum/COPYRIGHT",
        "Spectrum-Icons/COPYRIGHT",
    ),
    ("assets/icons/spectrum/NOTICE", "Spectrum-Icons/NOTICE"),
    (
        "assets/icons/spectrum/README.md",
        "Spectrum-Icons/README.md",
    ),
];

fn copy_third_party_notices(ws: &Workspace, dest: &Path) -> Result<()> {
    for (from, to) in THIRD_PARTY_NOTICES {
        let to = dest.join("ThirdPartyNotices").join(to);
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        copy_file(&ws.workspace_root.join(from), &to)?;
    }
    Ok(())
}

const BUNDLE_ID: &str = "io.github.felixdad.bae";
const BUNDLE_NAME: &str = "BAE";
const MIN_MACOS: &str = "12.0";
/// `Contents/Resources/<ICON_NAME>.icns` (`CFBundleIconFile`).
const ICON_NAME: &str = "bae";
/// Uniform type identifiers the bundle exports for its documents.
const PROJECT_UTI: &str = "io.github.felixdad.bae.project";
const PACKAGE_UTI: &str = "io.github.felixdad.bae.package";

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
    if target.is_macos() && !cfg!(target_os = "macos") {
        // Without the ad-hoc signature the bundle is reported as "damaged" once downloaded.
        bail!("packaging BAE.app needs `codesign`, so it has to run on macOS");
    }
    let version = ws.package(APP_PACKAGE)?.version.clone();

    if !build.no_build {
        ws.build_release_app(&target)?;
    }
    let binary = ws.existing_release_binary(&target)?;
    if target.is_windows() {
        check_exe_icon(&binary)?;
    }
    let dist = ws.target_directory.join("dist");
    fs::create_dir_all(&dist).with_context(|| format!("failed to create {}", dist.display()))?;
    let stamp = zip_timestamp();

    let (unpacked, archive) = if target.is_macos() {
        package_macos(
            &ws,
            &binary,
            &version,
            &target,
            &dist,
            stamp,
            Signing::AdHoc,
        )?
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
            "  note: BAE.app is ad-hoc signed and not notarized; open it the first time with \
             right-click > Open, or System Settings > Privacy & Security > Open Anyway."
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

/// How `package_macos` signs the bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Signing {
    /// Ad-hoc signature (`codesign --sign -`), then `codesign --verify --deep --strict`.
    AdHoc,
    /// Leave the bundle as assembled. Only for layout tests whose "binary" is not Mach-O.
    #[cfg(test)]
    None,
}

/// Builds `dist/BAE.app`, signs it and zips it. Returns (bundle dir, zip path).
fn package_macos(
    ws: &Workspace,
    binary: &Path,
    version: &str,
    target: &Target,
    dist: &Path,
    stamp: DateTime,
    signing: Signing,
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
        &icon::icon_file(&ws.workspace_root, "bae.icns"),
        &resources.join(format!("{ICON_NAME}.icns")),
    )?;
    copy_file(
        &ws.workspace_root.join("LICENSE"),
        &resources.join("LICENSE"),
    )?;
    copy_third_party_notices(ws, &resources)?;
    if signing == Signing::AdHoc {
        sign_bundle_ad_hoc(&app)?;
    }

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
    copy_third_party_notices(ws, &stage)?;
    let zip = dist.join(format!("{name}.zip"));
    zip_dir(&stage, "", &zip, stamp)?;
    Ok((stage, zip))
}

/// `bae.exe` must carry the app icon (`RT_GROUP_ICON`, from `assets/icons/app/bae.res`
/// through `apps/bae/build.rs`); otherwise Explorer and the taskbar show a blank one.
fn check_exe_icon(exe: &Path) -> Result<()> {
    let bytes = fs::read(exe).with_context(|| format!("failed to read {}", exe.display()))?;
    if !icon::pe_has_group_icon(&bytes).with_context(|| format!("reading {}", exe.display()))? {
        bail!(
            "{} has no icon resource; apps/bae/build.rs should link {}/bae.res",
            exe.display(),
            icon::ICON_DIR
        );
    }
    Ok(())
}

/// Ad-hoc signs the assembled bundle and verifies it.
///
/// On Apple Silicon the linker already gives the executable an ad-hoc signature, but that
/// signature does not seal `Info.plist` or `Resources/`. Inside a bundle it is therefore
/// invalid ("code has no resources but signature indicates they must be present"), and
/// Gatekeeper reports a downloaded copy as damaged with no way to open it (#105).
///
/// Signs inside out without `--deep`: every executable in `Contents/MacOS` first, then the
/// bundle itself (which re-signs the main executable with the bundle identifier and writes
/// `Contents/_CodeSignature/CodeResources`). No hardened runtime: it would turn on library
/// validation, and BAE loads unsigned plugin libraries. A Developer ID signature and
/// notarization replace this once there is an Apple developer account.
fn sign_bundle_ad_hoc(app: &Path) -> Result<()> {
    let macos = app.join("Contents").join("MacOS");
    let mut executables = fs::read_dir(&macos)
        .with_context(|| format!("failed to read {}", macos.display()))?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    executables.sort();
    for code in executables.iter().map(PathBuf::as_path).chain([app]) {
        util::run(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(code),
        )?;
    }
    verify_bundle_signature(app)
}

/// `codesign --verify --deep --strict`: the check Gatekeeper's "damaged" verdict comes from.
fn verify_bundle_signature(app: &Path) -> Result<()> {
    util::run(
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(app),
    )
    .context("BAE.app does not have a valid code signature")
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

enum Plist<'a> {
    String(&'a str),
    Bool(bool),
    Array(Vec<Plist<'a>>),
    Dict(Vec<(&'a str, Plist<'a>)>),
}

fn strings<'a>(items: &[&'a str]) -> Plist<'a> {
    Plist::Array(items.iter().map(|s| Plist::String(s)).collect())
}

/// One `CFBundleDocumentTypes` entry: BAE is the owner and editor of `uti`. The document
/// icon is composed by macOS from the app icon (`CFBundleTypeIconSystemGenerated`).
fn document_type<'a>(name: &'a str, uti: &'a str, package: bool) -> Plist<'a> {
    use Plist::{Bool, Dict, String as Str};
    let mut entries = vec![
        ("CFBundleTypeName", Str(name)),
        ("CFBundleTypeRole", Str("Editor")),
        ("LSHandlerRank", Str("Owner")),
        ("LSItemContentTypes", strings(&[uti])),
        ("CFBundleTypeIconSystemGenerated", Bool(true)),
    ];
    if package {
        entries.push(("LSTypeIsPackage", Bool(true)));
    }
    Dict(entries)
}

/// One `UTExportedTypeDeclarations` entry for a file name extension.
fn exported_type<'a>(
    uti: &'a str,
    description: &'a str,
    conforms_to: &[&'a str],
    extension: &'a str,
) -> Plist<'a> {
    use Plist::{Dict, String as Str};
    Dict(vec![
        ("UTTypeIdentifier", Str(uti)),
        ("UTTypeDescription", Str(description)),
        ("UTTypeConformsTo", strings(conforms_to)),
        (
            "UTTypeTagSpecification",
            Dict(vec![("public.filename-extension", strings(&[extension]))]),
        ),
    ])
}

/// The bundle's `Info.plist` (XML property list).
pub fn info_plist(version: &str) -> String {
    use Plist::{Array, Bool, String as Str};
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
        ("CFBundleIconFile", Str(ICON_NAME)),
        ("LSMinimumSystemVersion", Str(MIN_MACOS)),
        ("NSHighResolutionCapable", Bool(true)),
        ("NSSupportsAutomaticGraphicsSwitching", Bool(true)),
        (
            "LSApplicationCategoryType",
            Str("public.app-category.video"),
        ),
        ("NSHumanReadableCopyright", Str("Apache-2.0")),
        // A `.bae` project is a folder: as a package Finder shows it as one document
        // that opens in BAE on double-click. `.baez` is a single (zip) file.
        (
            "CFBundleDocumentTypes",
            Array(vec![
                document_type("BAE Project", PROJECT_UTI, true),
                document_type("BAE Project Package", PACKAGE_UTI, false),
            ]),
        ),
        (
            "UTExportedTypeDeclarations",
            Array(vec![
                exported_type(
                    PROJECT_UTI,
                    "BAE Project",
                    &["com.apple.package", "public.composite-content"],
                    "bae",
                ),
                exported_type(
                    PACKAGE_UTI,
                    "BAE Project Package",
                    &["public.data", "public.composite-content"],
                    "baez",
                ),
            ]),
        ),
    ];
    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n",
    ));
    write_plist(&mut xml, &Plist::Dict(entries.into()), 0);
    xml.push_str("</plist>\n");
    xml
}

/// Appends `value` as plist XML, one element per line, indented with tabs.
fn write_plist(xml: &mut String, value: &Plist<'_>, depth: usize) {
    let pad = "\t".repeat(depth);
    match value {
        Plist::String(s) => xml.push_str(&format!("{pad}<string>{}</string>\n", xml_escape(s))),
        Plist::Bool(true) => xml.push_str(&format!("{pad}<true/>\n")),
        Plist::Bool(false) => xml.push_str(&format!("{pad}<false/>\n")),
        Plist::Array(items) => {
            xml.push_str(&format!("{pad}<array>\n"));
            for item in items {
                write_plist(xml, item, depth + 1);
            }
            xml.push_str(&format!("{pad}</array>\n"));
        }
        Plist::Dict(entries) => {
            xml.push_str(&format!("{pad}<dict>\n"));
            for (key, item) in entries {
                xml.push_str(&format!("{pad}\t<key>{}</key>\n", xml_escape(key)));
                write_plist(xml, item, depth + 1);
            }
            xml.push_str(&format!("{pad}</dict>\n"));
        }
    }
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
            ("CFBundleIconFile", "<string>bae</string>"),
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
        // Every element on its own line and every container closed.
        for (open, close) in [("<dict>", "</dict>"), ("<array>", "</array>")] {
            assert_eq!(xml.matches(open).count(), xml.matches(close).count());
        }
        for uti in [PROJECT_UTI, PACKAGE_UTI] {
            // Declared as a document type and exported as a type.
            assert_eq!(xml.matches(&format!("<string>{uti}</string>")).count(), 2);
        }
        assert!(xml.contains("<key>LSTypeIsPackage</key>"));
        assert!(xml.contains("\t\t\t\t<string>com.apple.package</string>\n"));
    }

    /// macOS parses the plist and finds the document types where Launch Services
    /// looks for them (#115).
    #[cfg(target_os = "macos")]
    #[test]
    fn plutil_reads_the_document_types() {
        let path = scratch_dir("plist").join("Info.plist");
        fs::write(&path, info_plist("0.1.0")).unwrap();
        let plutil = |args: &[&str]| {
            let out = Command::new("plutil")
                .args(args)
                .arg(&path)
                .output()
                .unwrap();
            assert!(out.status.success(), "plutil {args:?}: {out:?}");
            String::from_utf8(out.stdout).unwrap().trim().to_owned()
        };
        plutil(&["-lint"]);
        let get = |key: &str| plutil(&["-extract", key, "raw", "-o", "-"]);
        assert_eq!(get("CFBundleIconFile"), "bae");
        assert_eq!(
            get("CFBundleDocumentTypes.0.LSItemContentTypes.0"),
            PROJECT_UTI
        );
        assert_eq!(get("CFBundleDocumentTypes.0.LSTypeIsPackage"), "true");
        assert_eq!(get("CFBundleDocumentTypes.0.CFBundleTypeRole"), "Editor");
        assert_eq!(
            get("CFBundleDocumentTypes.1.LSItemContentTypes.0"),
            PACKAGE_UTI
        );
        assert_eq!(
            get(
                "UTExportedTypeDeclarations.0.UTTypeTagSpecification.public\\.filename-extension.0"
            ),
            "bae"
        );
        assert_eq!(
            get("UTExportedTypeDeclarations.0.UTTypeConformsTo.0"),
            "com.apple.package"
        );
        assert_eq!(
            get(
                "UTExportedTypeDeclarations.1.UTTypeTagSpecification.public\\.filename-extension.0"
            ),
            "baez"
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
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
            Signing::None,
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
                dir("BAE.app/Contents/Resources/ThirdPartyNotices/"),
                dir("BAE.app/Contents/Resources/ThirdPartyNotices/Spectrum-Icons/"),
                file(
                    "BAE.app/Contents/Resources/ThirdPartyNotices/Spectrum-Icons/COPYRIGHT",
                    0o644
                ),
                file(
                    "BAE.app/Contents/Resources/ThirdPartyNotices/Spectrum-Icons/LICENSE",
                    0o644
                ),
                file(
                    "BAE.app/Contents/Resources/ThirdPartyNotices/Spectrum-Icons/NOTICE",
                    0o644
                ),
                file(
                    "BAE.app/Contents/Resources/ThirdPartyNotices/Spectrum-Icons/README.md",
                    0o644
                ),
                file("BAE.app/Contents/Resources/bae.icns", 0o644),
            ]
        );
    }

    /// The signed bundle verifies, and still verifies after a round trip through our zip
    /// writer and `ditto` (what Archive Utility does with a downloaded zip), #105.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_package_is_signed_and_survives_the_zip() {
        let tmp = scratch_dir("mac-sign");
        // Any Mach-O will do; a copy of a tiny system tool keeps the test fast.
        let binary = tmp.join("bae-input");
        fs::copy("/usr/bin/true", &binary).unwrap();
        let target = Target {
            triple: "aarch64-apple-darwin".into(),
            explicit: false,
        };
        let (app, zip) = package_macos(
            &test_workspace(&tmp),
            &binary,
            "0.1.0",
            &target,
            &tmp,
            dos_datetime(1_700_000_000),
            Signing::AdHoc,
        )
        .unwrap();
        verify_bundle_signature(&app).unwrap();
        let names: Vec<String> = zip_entries(&zip).into_iter().map(|e| e.0).collect();
        assert!(
            names.contains(&"BAE.app/Contents/_CodeSignature/CodeResources".to_owned()),
            "{names:?}"
        );

        let unpacked = tmp.join("unpacked");
        let status = Command::new("ditto")
            .args(["-x", "-k"])
            .arg(&zip)
            .arg(&unpacked)
            .status()
            .unwrap();
        assert!(status.success());
        let result = verify_bundle_signature(&unpacked.join("BAE.app"));
        fs::remove_dir_all(&tmp).unwrap();
        result.unwrap();
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
        assert_eq!(
            names,
            [
                "LICENSE",
                "README.md",
                "ThirdPartyNotices/",
                "ThirdPartyNotices/Spectrum-Icons/",
                "ThirdPartyNotices/Spectrum-Icons/COPYRIGHT",
                "ThirdPartyNotices/Spectrum-Icons/LICENSE",
                "ThirdPartyNotices/Spectrum-Icons/NOTICE",
                "ThirdPartyNotices/Spectrum-Icons/README.md",
                "bae.exe",
            ]
        );
    }
}
