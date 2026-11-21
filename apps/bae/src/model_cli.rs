//! `bae export-json`: print a project (or one sublayer / one prim) as readable JSON.
//!
//! Lives in the app because `bae-model` must not depend on `bae-platform` (where the
//! CLI registry is); the logic itself is `bae_model::export_*`.

use std::path::PathBuf;
use std::process::ExitCode;

use bae_app::{AppBuilder, AppModule};
use bae_model::{PrimPath, Project, SublayerId};
use bae_platform::cli::{self, CliSubcommand};
use clap::{Arg, ArgAction};

pub struct ModelCliModule;

impl AppModule for ModelCliModule {
    fn build(&self, app: &mut AppBuilder) {
        let registry = app.registry::<CliSubcommand>();
        let export = CliSubcommand::new("export-json", "Export a project as readable JSON")
            .with_args(|cmd| {
                cmd.arg(
                    Arg::new("project")
                        .value_name("PROJECT")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("A .bae directory or .baez file"),
                )
                .arg(
                    Arg::new("sublayer")
                        .long("sublayer")
                        .value_name("ID")
                        .help("Export one sublayer's own opinions instead of the composed stage"),
                )
                .arg(
                    Arg::new("prim")
                        .long("prim")
                        .value_name("PATH")
                        .help("Export only this prim of the composed stage"),
                )
                .arg(
                    Arg::new("out")
                        .long("out")
                        .short('o')
                        .value_name("FILE")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Write to FILE instead of stdout"),
                )
                .arg(
                    Arg::new("compact")
                        .long("compact")
                        .action(ArgAction::SetTrue)
                        .help("No indentation"),
                )
            })
            .with_run(|ctx| {
                let m = ctx.matches;
                let path = m.get_one::<PathBuf>("project").expect("required");
                let project = Project::open(path).map_err(|error| {
                    anyhow::anyhow!(crate::file_error::describe_open(path, &error.into()))
                })?;
                let json = match (m.get_one::<String>("sublayer"), m.get_one::<String>("prim")) {
                    (Some(id), _) => {
                        bae_model::export_sublayer_json(&project.stage, &SublayerId::new(id)?)?
                    }
                    (None, Some(prim)) => {
                        bae_model::export_prim_json(&project.stage, &PrimPath::new(prim)?)?
                    }
                    (None, None) => bae_model::export_flattened_json(&project.stage),
                };
                let text = if m.get_flag("compact") {
                    serde_json::to_string(&json)?
                } else {
                    serde_json::to_string_pretty(&json)?
                };
                match m.get_one::<PathBuf>("out") {
                    Some(out) => std::fs::write(out, text + "\n")?,
                    None => println!("{text}"),
                }
                Ok(ExitCode::SUCCESS)
            });
        if let Err(error) = cli::implement(&registry, app.current_owner(), export) {
            tracing::warn!("export-json not registered: {error}");
        }
    }
}
