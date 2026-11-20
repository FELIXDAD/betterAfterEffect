//! `bae mcp`: serve a project to an agent over MCP (stdio).
//!
//! Lives in the app (like `export-json`) because `bae-agent` defines commands and so
//! must not depend on `bae-platform`, where the CLI registry is. The server itself is
//! `bae_agent::run_stdio`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use bae_agent::{AuditLog, BaeMcpServer, InstructionsLang, McpOptions, Permission, Workspace};
use bae_app::{AppBuilder, AppModule};
use bae_model::{DynCommand, Project};
use bae_platform::AppDirs;
use bae_platform::cli::{self, CliSubcommand};
use clap::{Arg, ArgAction};

pub struct McpCliModule;

impl AppModule for McpCliModule {
    fn build(&self, app: &mut AppBuilder) {
        let registry = app.registry::<CliSubcommand>();
        let mcp = CliSubcommand::new(
            "mcp",
            "Run the MCP server for agents on stdin/stdout",
        )
        .with_args(|cmd| {
            cmd.arg(
                Arg::new("project")
                    .value_name("PROJECT")
                    .value_parser(clap::value_parser!(PathBuf))
                    .help("A .bae directory or .baez file (omitted: an unsaved, empty project)"),
            )
            .arg(
                Arg::new("create")
                    .long("create")
                    .action(ArgAction::SetTrue)
                    .help("Create PROJECT if it does not exist"),
            )
            .arg(
                Arg::new("name")
                    .long("name")
                    .value_name("NAME")
                    .help("Agent name: writes go to sublayer agent/NAME (default: from the MCP client)"),
            )
            .arg(
                Arg::new("permission")
                    .long("permission")
                    .value_name("LEVEL")
                    .default_value("own-layer")
                    .value_parser(["read-only", "own-layer", "any-layer"])
                    .help("What the agent may write"),
            )
            .arg(
                Arg::new("audit-log")
                    .long("audit-log")
                    .value_name("FILE")
                    .value_parser(clap::value_parser!(PathBuf))
                    .help("Append the audit log here (default: agent-audit.jsonl in the log directory)"),
            )
            .arg(
                Arg::new("no-autosave")
                    .long("no-autosave")
                    .action(ArgAction::SetTrue)
                    .help("Do not save the project after every edit"),
            )
            .arg(
                Arg::new("instructions")
                    .long("instructions")
                    .value_name("LANG")
                    .default_value("en")
                    .value_parser(["en", "zh"])
                    .help("Language of the operating rules sent to the agent"),
            )
        })
        .with_run(|ctx| {
            let m = ctx.matches;
            let project = match m.get_one::<PathBuf>("project") {
                Some(path) if !path.exists() && m.get_flag("create") => {
                    let mut project = Project::new();
                    project.save_as(path)?;
                    project
                }
                Some(path) => Project::open(path)?,
                None => {
                    tracing::warn!("no project given: serving an unsaved, empty project");
                    Project::new()
                }
            };
            let permission: Permission = m
                .get_one::<String>("permission")
                .expect("has default")
                .parse()
                .map_err(anyhow::Error::msg)?;
            let lang: InstructionsLang = m
                .get_one::<String>("instructions")
                .expect("has default")
                .parse()
                .map_err(anyhow::Error::msg)?;
            let audit_path = m.get_one::<PathBuf>("audit-log").cloned().or_else(|| {
                ctx.app
                    .resource::<AppDirs>()
                    .map(|d| d.logs.join("agent-audit.jsonl"))
            });
            let audit = match &audit_path {
                Some(path) => AuditLog::with_file(path).unwrap_or_else(|e| {
                    tracing::warn!("audit log {} unavailable: {e}", path.display());
                    AuditLog::new()
                }),
                None => AuditLog::new(),
            };
            let options = McpOptions {
                agent_name: m.get_one::<String>("name").cloned(),
                permission,
                autosave: !m.get_flag("no-autosave"),
                lang,
                instructions: None,
            };
            if let Some(name) = &options.agent_name {
                bae_agent::AgentIdentity::new(name)?;
            }
            // Domain commands that app modules published (bae-motion's comp.create,
            // layer.create …), on top of the model, agent and packaging commands.
            let module_commands: Vec<Arc<dyn DynCommand>> = ctx
                .app
                .registry::<dyn DynCommand>()
                .snapshot()
                .iter()
                .map(|e| e.item().clone())
                .collect();
            let workspace = Workspace::with_commands(project, module_commands);
            let server = BaeMcpServer::new(workspace.shared(), options)
                .with_audit_log(Arc::new(audit));
            bae_agent::run_stdio(server)?;
            Ok(ExitCode::SUCCESS)
        });
        if let Err(error) = cli::implement(&registry, app.current_owner(), mcp) {
            tracing::warn!("mcp not registered: {error}");
        }
    }
}
