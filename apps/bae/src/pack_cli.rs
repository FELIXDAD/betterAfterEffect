//! `bae pack <PROJECT>`: package a project and every asset file it uses into
//! one self-contained `.baez` (workstream 21's `bae_package::pack`). The project
//! and its files are not changed; the package keeps the project's manifest
//! fields, including the saved view (open comps), so it opens where the project
//! was left.
//!
//! ```text
//! bae pack Film.bae                  → Film.baez next to it
//! bae pack Film.bae -o out.baez --overwrite --used-only --json
//! ```
//!
//! Errors name the file and say what is wrong, in the UI language
//! (`file_error`), like `bae export-json` and `bae render`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bae_app::{AppBuilder, AppModule};
use bae_package::{PackOptions, PackSource, PackageError, WriteOptions};
use bae_platform::cli::{self, CliSubcommand};
use clap::{Arg, ArgAction};

use crate::document::Document;
use crate::i18n::tr;

/// Implements the reserved `pack` subcommand.
#[derive(Debug, Default)]
pub struct PackCliModule;

impl AppModule for PackCliModule {
    fn name(&self) -> &str {
        "bae-pack-cli"
    }

    fn build(&self, app: &mut AppBuilder) {
        let registry = app.registry::<CliSubcommand>();
        let pack = CliSubcommand::new("pack", "Pack a project and its assets into a .baez file")
            .with_args(|cmd| {
                cmd.arg(
                    Arg::new("project")
                        .value_name("PROJECT")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("A .bae folder or .baez file"),
                )
                .arg(
                    Arg::new("out")
                        .long("out")
                        .short('o')
                        .value_name("FILE")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("The .baez to write (default: next to the project, same name)"),
                )
                .arg(
                    Arg::new("overwrite")
                        .long("overwrite")
                        .action(ArgAction::SetTrue)
                        .help("Replace an existing file"),
                )
                .arg(
                    Arg::new("used-only")
                        .long("used-only")
                        .action(ArgAction::SetTrue)
                        .help("Leave out assets that no composition uses"),
                )
                .arg(
                    Arg::new("fail-on-missing")
                        .long("fail-on-missing")
                        .action(ArgAction::SetTrue)
                        .help("Fail instead of packing when asset files are missing"),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help("Print the package report as JSON"),
                )
            })
            .with_run(|ctx| {
                let m = ctx.matches;
                let project = m.get_one::<PathBuf>("project").expect("required");
                let out = m
                    .get_one::<PathBuf>("out")
                    .map(|o| baez_path(o.clone()))
                    .unwrap_or_else(|| default_output(project));
                let options = PackOptions {
                    include_unused: !m.get_flag("used-only"),
                    fail_on_missing: m.get_flag("fail-on-missing"),
                    ..PackOptions::default()
                };
                let write = WriteOptions {
                    overwrite: m.get_flag("overwrite"),
                };
                let report = pack(project, &out, &options, write)?;
                if m.get_flag("json") {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    let s = &report.stats;
                    println!(
                        "{} {}: {} {}, {:.1} MiB",
                        tr("packed"),
                        out.display(),
                        s.files,
                        tr("asset file(s)"),
                        s.output_bytes.unwrap_or(0) as f64 / (1024.0 * 1024.0)
                    );
                    for w in &report.warnings {
                        eprintln!("{}: {}", tr("warning"), w.message);
                    }
                }
                Ok(ExitCode::SUCCESS)
            });
        if let Err(error) = cli::implement(&registry, app.current_owner(), pack) {
            tracing::warn!("pack not registered: {error}");
        }
    }
}

/// Open `project` and write the package to `out`.
pub fn pack(
    project: &Path,
    out: &Path,
    options: &PackOptions,
    write: WriteOptions,
) -> anyhow::Result<bae_package::PackageReport> {
    let opened = Document::load(project)
        .map_err(|error| anyhow::anyhow!(crate::file_error::describe_open(project, &error)))?;
    bae_package::pack(&PackSource::project(&opened.project), out, options, write)
        .map_err(|error| anyhow::anyhow!(describe_pack_error(out, error)))
}

/// `Film.bae` → `Film.baez` next to it (`Film-packed.baez` when the input is
/// already that file).
fn default_output(project: &Path) -> PathBuf {
    let project = match project.parent() {
        Some(dir) if project.file_name().is_some_and(|n| n == "project.json") => dir,
        _ => project,
    };
    let dir = project.parent().unwrap_or(Path::new(""));
    let stem = project
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_owned());
    let out = dir.join(format!("{stem}.baez"));
    if out == project {
        dir.join(format!("{stem}-packed.baez"))
    } else {
        out
    }
}

/// The package always gets the `.baez` extension.
fn baez_path(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("baez"))
    {
        path.with_extension("baez")
    } else {
        let mut name = path.into_os_string();
        name.push(".baez");
        PathBuf::from(name)
    }
}

/// What went wrong writing the package, naming the file.
fn describe_pack_error(out: &Path, error: PackageError) -> String {
    let file = out.display().to_string();
    let sentence = match &error {
        PackageError::OutputExists(_) => {
            tr("“{file}” already exists; use --overwrite to replace it.").replace("{file}", &file)
        }
        PackageError::OutputIsSource(_) => {
            tr("“{file}” would overwrite the project or one of its assets.")
                .replace("{file}", &file)
        }
        PackageError::MissingAssets(list) => {
            tr("{n} asset file(s) are missing; nothing was written.")
                .replace("{n}", &list.len().to_string())
        }
        _ => return crate::file_error::describe_save(out, &anyhow::Error::new(error)),
    };
    format!("{sentence}\n{} {error}", tr("Details:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names() {
        assert_eq!(
            default_output(Path::new("/p/Film.bae")),
            PathBuf::from("/p/Film.baez")
        );
        assert_eq!(
            default_output(Path::new("/p/Film.baez")),
            PathBuf::from("/p/Film-packed.baez")
        );
        assert_eq!(
            default_output(Path::new("/p/Film.bae/project.json")),
            PathBuf::from("/p/Film.baez")
        );
        assert_eq!(baez_path("a".into()), PathBuf::from("a.baez"));
        assert_eq!(baez_path("a.BAEZ".into()), PathBuf::from("a.baez"));
        assert_eq!(baez_path("a.zip".into()), PathBuf::from("a.zip.baez"));
    }
}
