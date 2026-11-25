//! # bae-script — After Effects-style scripts on QuickJS
//!
//! Runs JavaScript written against the common subset of the AE scripting object
//! model (`app.project`, items, `CompItem`, layers, `property()` / `setValue` /
//! `setValueAtTime` / keyframes, `layers.addSolid` / `addText` / `addNull`,
//! `app.beginUndoGroup`) on the QuickJS engine the expression engine already
//! links (`bae-anim`), so the binary keeps one JS engine.
//!
//! - **Every change is a command.** The object model calls registered
//!   [`DynCommand`]s by id (`layer.create`, `attr.set_at`, `attr.set_animated`,
//!   `key.delete`, `effect.apply` …): the same commands menus, shortcuts and MCP
//!   tools run (ARCHITECTURE §2 rule 1). Scripts can also call any command with
//!   `bae.commands.invoke(id, params)`.
//! - **One run = one undo step.** The whole run is one `Stage` transaction;
//!   `app.beginUndoGroup(name)` names it, `endUndoGroup` does nothing (several
//!   groups in one run stay one step). Reads see the run's own changes.
//! - **Errors keep earlier changes** (like AE): the run stops, the error and its
//!   line are reported in [`ScriptReport::error`], and Edit ▸ Undo reverts the
//!   whole run.
//! - **Limits:** 128 MiB of JS heap and 30 s per run; scripts never render.
//!
//! The command [`RunScript`] (`script.run`) runs a script from menus (File ▸
//! Scripts), `bae --headless` and MCP agents; [`ScriptModule`] publishes it.
//! User documentation (Chinese): `docs/plugins/scripting.md`.

mod convert;
mod engine;
mod host;

pub use engine::check_module;

use std::sync::{Arc, Mutex};

use bae_app::{AppBuilder, AppModule, Registry};
use bae_model::{
    AuthorId, CommandError, CommandRegistry, DynCommand, EffectDef, PrimPath, Stage, StageEvent,
    Time, Transaction,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Id of [`RunScript`].
pub const RUN_SCRIPT: &str = "script.run";

/// What a script run produced.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptReport {
    /// Script name (file name), for messages.
    pub name: String,
    /// `console.log` / `$.writeln` output, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
    /// `alert()` messages, in order (the app shows them after the run).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<String>,
    /// The error that stopped the script, with its line (`TypeError at line 3:
    /// …`). Changes made before it are kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The script's completion value (its last expression) as JSON; layers and
    /// items as `{"layer"|"item": path, "name": …}`.
    #[serde(default)]
    pub result: serde_json::Value,
    /// The name passed to the first `app.beginUndoGroup`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo_group: Option<String>,
}

/// Looks up a command by id.
pub type CommandLookup<'a> = dyn Fn(&str) -> Option<Arc<dyn DynCommand>> + 'a;
/// Looks up an effect definition by match name (or display name).
pub type EffectLookup<'a> = dyn Fn(&str) -> Option<Arc<EffectDef>> + 'a;

/// What a script can reach besides the stage.
pub struct ScriptEnv<'a> {
    /// The commands scripts run.
    pub commands: &'a CommandLookup<'a>,
    /// Effect definitions (`layer.effect(...)`, `Effects.addProperty`).
    pub effects: &'a EffectLookup<'a>,
    /// `app.project.activeItem`.
    pub active_comp: Option<PrimPath>,
    /// `comp.selectedLayers`.
    pub selected_layers: Vec<PrimPath>,
    /// The current time (comp time): `comp.time`, `Property.value`.
    pub time: Time,
    /// How long the run may take (default [`TIME_LIMIT`]).
    pub time_limit: Option<std::time::Duration>,
}

/// Default time limit of a run.
pub const TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Runs `source` inside the open transaction `tx` (its changes join that undo
/// step). `name` is used in messages.
pub fn run_in(
    tx: &mut Transaction<'_>,
    env: &ScriptEnv<'_>,
    name: &str,
    source: &str,
) -> ScriptReport {
    run_program(
        tx,
        env,
        name,
        engine::Program::File(source),
        ProjectAccess::Write,
    )
}

/// Project access granted to one script invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectAccess {
    None,
    Read,
    Write,
}

/// The active composition, layer selection and playhead at invocation time.
#[derive(Default)]
pub struct ScriptSelection {
    pub comp: Option<PrimPath>,
    pub layers: Vec<PrimPath>,
    pub time: Time,
}

/// Supplies view state without coupling the plugin host to a workspace.
pub trait ScriptView: std::fmt::Debug + Send + Sync {
    fn selection(&self) -> ScriptSelection;
}

/// An exported function of an installed ES module.
pub struct ModuleEntry<'a> {
    pub source: &'a str,
    pub export: &'a str,
    pub args: &'a serde_json::Value,
    pub heap_bytes: usize,
}

/// Executes an exported function in the caller's transaction.
pub fn run_entry(
    tx: &mut Transaction<'_>,
    env: &ScriptEnv<'_>,
    name: &str,
    entry: ModuleEntry<'_>,
    access: ProjectAccess,
) -> ScriptReport {
    run_program(tx, env, name, engine::Program::Module(entry), access)
}

fn run_program(
    tx: &mut Transaction<'_>,
    env: &ScriptEnv<'_>,
    name: &str,
    program: engine::Program<'_>,
    access: ProjectAccess,
) -> ScriptReport {
    let mut state = host::RunState {
        tx,
        env,
        access,
        selected: env.selected_layers.clone(),
        report: ScriptReport {
            name: name.to_owned(),
            ..ScriptReport::default()
        },
    };
    match engine::execute(&mut state, program) {
        Ok(result) => state.report.result = result,
        Err(error) => state.report.error = Some(error),
    }
    state.report
}

/// Runs `source` as one undo step by `author`, labelled with the first
/// `app.beginUndoGroup` name (else "Run Script: <name>").
pub fn run(
    stage: &mut Stage,
    author: &AuthorId,
    env: &ScriptEnv<'_>,
    name: &str,
    source: &str,
) -> Result<(ScriptReport, StageEvent), CommandError> {
    // The label is fixed when the step starts, so a literal group name is read
    // from the source.
    let label = undo_group_hint(source).unwrap_or_else(|| format!("Run Script: {name}"));
    stage.transact(author, &label, |tx| {
        Ok::<_, CommandError>(run_in(tx, env, name, source))
    })
}

/// The literal name of the first `app.beginUndoGroup("…")` call, so the undo
/// step can carry it from the start.
fn undo_group_hint(source: &str) -> Option<String> {
    let at = source.find("beginUndoGroup(")? + "beginUndoGroup(".len();
    let rest = source[at..].trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let body = &rest[1..];
    let end = body.find(quote)?;
    Some(body[..end].to_owned()).filter(|s| !s.is_empty())
}

/// Every built-in command a script can use outside the app: the model commands
/// and the motion commands, with the effect commands over `defs`.
pub fn standard_commands(defs: Arc<Registry<EffectDef>>) -> CommandRegistry {
    let mut r = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut r);
    for cmd in bae_motion::effect::effect_commands(defs) {
        let _ = r.register_dyn(cmd);
    }
    r
}

/// Looks up an effect definition by match name, else by display name (AE's
/// `addProperty` and `layer.effect()` accept both).
pub fn find_effect(defs: &Registry<EffectDef>, name: &str) -> Option<Arc<EffectDef>> {
    let snapshot = defs.snapshot();
    let all = || snapshot.iter().map(|e| e.item());
    all()
        .find(|d| d.match_name == name)
        .or_else(|| all().find(|d| d.name.eq_ignore_ascii_case(name)))
        .cloned()
}

/// The last report of a `script.run` from the UI, for the app to show (alerts,
/// errors). Shared by [`RunScript`] and the UI as an app resource.
#[derive(Clone, Debug, Default)]
pub struct ScriptConsole(Arc<Mutex<Option<ScriptReport>>>);

impl ScriptConsole {
    /// Takes the last report.
    pub fn take(&self) -> Option<ScriptReport> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    fn put(&self, report: ScriptReport) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
    }
}

/// Parameters of `script.run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunScriptParams {
    /// JavaScript source using the After Effects scripting object model (app.project,
    /// items, layers, property(), setValue, setValueAtTime, layers.addSolid/addText/addNull …).
    pub source: String,
    /// Script name for messages, e.g. the file name.
    #[serde(default)]
    pub name: Option<String>,
    /// The comp `app.project.activeItem` returns.
    #[serde(default)]
    pub active_comp: Option<PrimPath>,
    /// The layers `comp.selectedLayers` returns.
    #[serde(default)]
    pub selected_layers: Vec<PrimPath>,
    /// Current time in seconds of comp time (`comp.time`, `Property.value`); default 0.
    #[serde(default)]
    pub time: Option<f64>,
}

/// `script.run`: runs a script as one undo step. The report (log, alerts, the
/// error and its line, the completion value) is the command's output; a script
/// error does not fail the command, so changes made before it are kept.
pub struct RunScript {
    /// Module commands (`bae-motion` …) published in the app.
    app: std::sync::Weak<Registry<dyn DynCommand>>,
    /// Model commands and, without an app registry, the motion commands.
    builtins: CommandRegistry,
    defs: Arc<Registry<EffectDef>>,
    console: ScriptConsole,
}

impl RunScript {
    /// `script.run` over the app's command registry (module commands) plus the
    /// model commands.
    pub fn new(
        app: Arc<Registry<dyn DynCommand>>,
        defs: Arc<Registry<EffectDef>>,
        console: ScriptConsole,
    ) -> Self {
        RunScript {
            app: Arc::downgrade(&app),
            builtins: CommandRegistry::with_builtins(),
            defs,
            console,
        }
    }

    /// `script.run` with the standard commands only ([`standard_commands`]).
    pub fn standalone(defs: Arc<Registry<EffectDef>>) -> Self {
        RunScript {
            app: std::sync::Weak::new(),
            builtins: standard_commands(defs.clone()),
            defs,
            console: ScriptConsole::default(),
        }
    }

    /// Reports go to `console` (the UI's).
    pub fn with_console(mut self, console: ScriptConsole) -> Self {
        self.console = console;
        self
    }

    fn command(&self, id: &str) -> Option<Arc<dyn DynCommand>> {
        self.app
            .upgrade()
            .and_then(|r| r.get(id))
            .or_else(|| self.builtins.get(id).cloned())
    }
}

impl bae_model::Command for RunScript {
    type Params = RunScriptParams;
    type Output = ScriptReport;
    const ID: &'static str = RUN_SCRIPT;
    const TITLE: &'static str = "Run Script";
    const DESCRIPTION: &'static str = "Run a JavaScript using the After Effects scripting object model (app.project.activeItem, comp.layers, layer.property(\"Position\").setValue / setValueAtTime, layers.addSolid / addText / addNull, bae.commands.invoke(id, params) …) as one undo step. Every change goes through BAE commands. Returns the log, alert texts, the error with its line (changes made before an error are kept) and the script's last expression value.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: RunScriptParams,
    ) -> Result<ScriptReport, CommandError> {
        let commands = |id: &str| self.command(id);
        let effects = |name: &str| find_effect(&self.defs, name);
        let env = ScriptEnv {
            commands: &commands,
            effects: &effects,
            active_comp: p.active_comp,
            selected_layers: p.selected_layers,
            time: Time::from_seconds(p.time.unwrap_or(0.0)),
            time_limit: None,
        };
        let name = p.name.unwrap_or_else(|| "script".to_owned());
        let report = run_in(tx, &env, &name, &p.source);
        self.console.put(report.clone());
        Ok(report)
    }
}

/// App module: publishes `script.run` in `Registry<dyn DynCommand>` (so menus,
/// `bae --headless` and MCP agents can run scripts) and the [`ScriptConsole`]
/// resource the UI reads reports from.
#[derive(Debug, Default)]
pub struct ScriptModule;

impl AppModule for ScriptModule {
    fn name(&self) -> &str {
        "bae-script"
    }

    fn build(&self, app: &mut AppBuilder) {
        let console = match app.resource::<ScriptConsole>() {
            Some(c) => c.clone(),
            None => {
                let c = ScriptConsole::default();
                app.insert_resource(c.clone());
                c
            }
        };
        let owner = app.current_owner();
        let defs = app.registry::<EffectDef>();
        let commands = app.registry::<dyn DynCommand>();
        // Through a command registry: it builds the type-erased command and
        // its parameter schema (the MCP tool definition).
        let mut r = CommandRegistry::new();
        let _ = r.register(RunScript::new(commands.clone(), defs, console));
        if let Some(cmd) = r.get(RUN_SCRIPT).cloned() {
            commands.batch(|tx| {
                if !tx.contains(RUN_SCRIPT) {
                    let _ = tx.register_arc(owner.clone(), RUN_SCRIPT, cmd);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_group_names_are_found() {
        assert_eq!(
            undo_group_hint("app.beginUndoGroup(\"Bounce\");"),
            Some("Bounce".into())
        );
        assert_eq!(
            undo_group_hint("app.beginUndoGroup( 'x' )"),
            Some("x".into())
        );
        assert_eq!(undo_group_hint("app.beginUndoGroup(name)"), None);
        assert_eq!(undo_group_hint("var a = 1;"), None);
    }
}
