//! One agent's session: identity, permission, edit target, and guarded execution.
//!
//! Permission is enforced by the transaction's write guard
//! ([`bae_model::Stage::transact_guarded`]): any op on, new sublayer named, or
//! edit-target switch to a sublayer the agent may not write fails *before* it is
//! written, and the whole call is rolled back. This covers commands that write
//! sublayers other than the edit target (namespace edits, `opinion.move`,
//! `sublayer.merge`, plugin commands) without knowing what they do, and a refused
//! call leaves no trace in other people's sublayers (not even in their CRDT history).

use std::cell::Cell;
use std::sync::Arc;

use bae_model::{CommandError, DynCommand, ModelError, ProjectError, StageEvent, SublayerId};
use serde_json::Value as Json;

use crate::identity::{AgentIdentity, Permission};
use crate::workspace::Workspace;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Refused by the agent's permission level; nothing was changed.
    #[error("permission denied: {0}")]
    Denied(String),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("{0}")]
    Invalid(String),
}

impl AgentError {
    pub fn is_denied(&self) -> bool {
        matches!(self, AgentError::Denied(_))
    }
}

/// One command invocation inside [`AgentSession::execute`].
#[derive(Clone, Debug)]
pub struct Step {
    /// Command ID, e.g. `attr.set`.
    pub command: String,
    pub params: Json,
}

/// Result of a successful [`AgentSession::execute`].
#[derive(Clone, Debug)]
pub struct ExecOutcome {
    /// One output per step.
    pub outputs: Vec<Json>,
    pub event: StageEvent,
}

/// Error inside the guarded transaction: which step failed, and why.
struct TxError {
    step: usize,
    error: CommandError,
}

impl From<ModelError> for TxError {
    fn from(e: ModelError) -> Self {
        TxError {
            step: usize::MAX,
            error: e.into(),
        }
    }
}

/// An agent connected to a workspace.
#[derive(Debug)]
pub struct AgentSession {
    identity: AgentIdentity,
    permission: Permission,
    /// Where this agent's edits go. `None` until the first write, which creates the
    /// proposal sublayer.
    edit_target: Option<SublayerId>,
}

impl AgentSession {
    pub fn new(identity: AgentIdentity, permission: Permission) -> Self {
        AgentSession {
            identity,
            permission,
            edit_target: None,
        }
    }

    pub fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    pub fn permission(&self) -> Permission {
        self.permission
    }

    pub fn set_permission(&mut self, permission: Permission) {
        self.permission = permission;
    }

    /// The sublayer this agent's edits are written to (its proposal layer unless it
    /// switched with `sublayer.set_edit_target`).
    pub fn edit_target(&self) -> &SublayerId {
        self.edit_target
            .as_ref()
            .unwrap_or_else(|| self.identity.proposal_layer())
    }

    /// Whether this agent may write `id` at all.
    pub fn may_write(&self, id: &SublayerId) -> bool {
        match self.permission {
            Permission::ReadOnly => false,
            Permission::OwnLayer => self.identity.owns(id),
            Permission::AnyLayer => true,
        }
    }

    fn require_write(&self) -> Result<(), AgentError> {
        if self.permission.can_write() {
            Ok(())
        } else {
            Err(AgentError::Denied(format!(
                "agent {} is read-only; it can query and render but not edit",
                self.identity.name()
            )))
        }
    }

    /// Make sure the edit target exists, creating the proposal sublayer (strongest
    /// position) through the `sublayer.create` command if needed.
    fn ensure_target(&mut self, ws: &mut Workspace) -> Result<SublayerId, AgentError> {
        let stage = &ws.project.stage;
        let target = self.edit_target().clone();
        if stage.sublayer(&target).is_some() {
            return Ok(target);
        }
        let proposal = self.identity.proposal_layer().clone();
        if stage.sublayer(&proposal).is_none() {
            let params = serde_json::json!({ "id": proposal, "index": 0 });
            if ws.registry.get("sublayer.create").is_some() {
                ws.registry.execute(
                    &mut ws.project.stage,
                    self.identity.author(),
                    "sublayer.create",
                    params,
                )?;
            } else {
                ws.project.stage.create_sublayer(proposal.clone(), 0)?;
            }
        }
        self.edit_target = Some(proposal.clone());
        Ok(proposal)
    }

    /// Run `steps` as one undo step by this agent, writing to its edit target and
    /// enforcing its permission. On any error nothing is changed.
    pub fn execute(
        &mut self,
        ws: &mut Workspace,
        label: Option<&str>,
        steps: &[Step],
    ) -> Result<ExecOutcome, AgentError> {
        self.require_write()?;
        if steps.is_empty() {
            return Err(AgentError::Invalid("no commands to run".into()));
        }
        let commands: Vec<Arc<dyn DynCommand>> = steps
            .iter()
            .map(|s| {
                ws.registry
                    .get(&s.command)
                    .cloned()
                    .ok_or_else(|| CommandError::Unknown(s.command.clone()).into())
            })
            .collect::<Result<_, AgentError>>()?;
        let label = label
            .map(str::to_owned)
            .unwrap_or_else(|| commands[0].info().title.clone());

        // Commands with an optional top-level `project` parameter get the open
        // project's location (the package.* commands only see the stage).
        let location = ws
            .project
            .location()
            .map(|l| Json::String(l.to_string_lossy().into_owned()));
        let params: Vec<Json> = steps
            .iter()
            .zip(&commands)
            .map(|(step, cmd)| {
                let mut params = step.params.clone();
                if let (Some(loc), Json::Object(obj)) = (&location, &mut params)
                    && !obj.contains_key("project")
                    && crate::tools::takes_project(&cmd.info().params_schema)
                {
                    obj.insert("project".into(), loc.clone());
                }
                params
            })
            .collect();

        let target = self.ensure_target(ws)?;
        let stage = &mut ws.project.stage;
        let restore_target = stage.edit_target().clone();
        stage.set_edit_target(&target)?;

        let stack_before = stage.layer_stack().to_vec();
        // The first sublayer the guard refused, for the error message (a command may
        // also turn the model error into its own message).
        let denied: Cell<Option<SublayerId>> = Cell::new(None);
        let allow = |id: &SublayerId| {
            let ok = self.may_write(id);
            if !ok {
                let first = denied.take().unwrap_or_else(|| id.clone());
                denied.set(Some(first));
            }
            ok
        };
        let result = stage.transact_guarded(self.identity.author(), &label, &allow, |tx| {
            let mut outputs = Vec::with_capacity(steps.len());
            for (i, (cmd, params)) in commands.iter().zip(params).enumerate() {
                // Frame numbers / timecode -> flicks, against the stage as it is now
                // (earlier commands of a batch may have created the comp).
                let schema = &cmd.info().params_schema;
                let params =
                    crate::timeargs::convert(tx.stage(), schema, params).map_err(|message| {
                        TxError {
                            step: i,
                            error: CommandError::InvalidParams {
                                command: cmd.info().id.clone(),
                                message,
                            },
                        }
                    })?;
                let out = cmd
                    .run_json(tx, params)
                    .map_err(|error| TxError { step: i, error })?;
                outputs.push(out);
            }
            Ok::<_, TxError>(outputs)
        });

        let new_target = stage.edit_target().clone();
        match result {
            Ok((outputs, event)) => {
                if new_target != target {
                    self.edit_target = Some(new_target);
                }
                let _ = stage.set_edit_target(&restore_target);
                Ok(ExecOutcome { outputs, event })
            }
            Err(TxError { step, error }) => {
                // Stack edits are not part of the transaction: drop sublayers the failed
                // call added (they are empty after the rollback).
                let added: Vec<SublayerId> = stage
                    .layer_stack()
                    .iter()
                    .filter(|id| !stack_before.contains(id))
                    .cloned()
                    .collect();
                for id in added {
                    let _ = stage.remove_sublayer(&id);
                }
                let _ = stage.set_edit_target(&restore_target);
                Err(match denied.take() {
                    Some(id) => {
                        let command = steps.get(step).map_or("the batch", |s| s.command.as_str());
                        AgentError::Denied(self.denial_message(command, &id))
                    }
                    None => error.into(),
                })
            }
        }
    }

    fn denial_message(&self, command: &str, sublayer: &SublayerId) -> String {
        format!(
            "{command} would write sublayer {sublayer}, but with {} permission agent {} may \
             only write its own sublayer {} (and agent/{}/...). Nothing was changed. \
             Delete/move/rename act on every sublayer that has the prim, so use them only \
             on prims you created; ask the user for anything else",
            self.permission,
            self.identity.name(),
            self.identity.proposal_layer(),
            self.identity.name(),
        )
    }

    /// Undo this agent's most recent step. `Ok(None)` if there is nothing to undo.
    pub fn undo(&mut self, ws: &mut Workspace) -> Result<Option<StageEvent>, AgentError> {
        self.require_write()?;
        Ok(ws.project.stage.undo(self.identity.author()))
    }

    /// Redo this agent's most recently undone step.
    pub fn redo(&mut self, ws: &mut Workspace) -> Result<Option<StageEvent>, AgentError> {
        self.require_write()?;
        Ok(ws.project.stage.redo(self.identity.author()))
    }
}

#[cfg(test)]
mod tests {
    use bae_model::{PrimPath, Project, export_flattened_json};
    use serde_json::json;

    use super::*;

    fn step(command: &str, params: Json) -> Step {
        Step {
            command: command.into(),
            params,
        }
    }

    /// A workspace whose root sublayer defines `/comp` (Comp) with `/comp/title`.
    fn workspace() -> Workspace {
        let mut ws = Workspace::new(Project::new());
        let user = bae_model::AuthorId::new("user:felix").unwrap();
        for (path, ty) in [("/comp", "Comp"), ("/comp/title", "Layer")] {
            ws.registry
                .execute(
                    &mut ws.project.stage,
                    &user,
                    "prim.create",
                    json!({ "path": path, "prim_type": ty }),
                )
                .unwrap();
        }
        ws
    }

    fn snapshot(ws: &Workspace) -> (Vec<SublayerId>, Json, Vec<bae_model::loro::VersionVector>) {
        let stage = &ws.project.stage;
        (
            stage.layer_stack().to_vec(),
            export_flattened_json(stage),
            // Versions of other people's sublayers: refused calls must not even add
            // history there. (A rolled-back batch may leave history in the agent's
            // own layer, like any failed command.)
            stage
                .sublayers()
                .filter(|l| !l.id().as_str().starts_with("agent/"))
                .map(|l| l.version())
                .collect(),
        )
    }

    fn session(p: Permission) -> AgentSession {
        AgentSession::new(AgentIdentity::new("claude").unwrap(), p)
    }

    #[test]
    fn writes_go_to_the_proposal_layer() {
        let mut ws = workspace();
        let mut s = session(Permission::OwnLayer);
        let out = s
            .execute(
                &mut ws,
                None,
                &[step(
                    "attr.set",
                    json!({ "path": "/comp/title", "attr": "opacity", "value": {"type": "float", "value": 50.0} }),
                )],
            )
            .unwrap();
        let own = SublayerId::new("agent/claude").unwrap();
        assert_eq!(out.event.sublayers, vec![own.clone()]);
        let stage = &ws.project.stage;
        assert_eq!(stage.layer_stack()[0], own);
        let title = stage.prim(&PrimPath::new("/comp/title").unwrap()).unwrap();
        assert_eq!(title.attr_source("opacity"), Some(&own));
        // The stage's own edit target is restored for other users.
        assert_eq!(stage.edit_target(), &SublayerId::root());
        // Undo is per agent.
        assert!(s.undo(&mut ws).unwrap().is_some());
        let title = ws
            .project
            .stage
            .prim(&PrimPath::new("/comp/title").unwrap())
            .unwrap();
        assert!(title.attr("opacity").is_none());
    }

    #[test]
    fn read_only_cannot_change_anything() {
        let mut ws = workspace();
        let before = snapshot(&ws);
        let mut s = session(Permission::ReadOnly);
        let infos: Vec<String> = ws.registry.infos().map(|i| i.id.clone()).collect();
        for id in infos {
            let err = s
                .execute(&mut ws, None, &[step(&id, json!({}))])
                .unwrap_err();
            assert!(err.is_denied(), "{id}: {err}");
        }
        assert!(s.undo(&mut ws).unwrap_err().is_denied());
        assert!(s.redo(&mut ws).unwrap_err().is_denied());
        assert_eq!(snapshot(&ws), before);
    }

    #[test]
    fn own_layer_cannot_touch_other_sublayers() {
        let mut ws = workspace();
        let mut s = session(Permission::OwnLayer);
        // Creates the proposal layer.
        s.execute(
            &mut ws,
            None,
            &[step(
                "prim.create",
                json!({ "path": "/comp/mine", "prim_type": "Layer" }),
            )],
        )
        .unwrap();
        let before = snapshot(&ws);
        let denied = [
            step("prim.delete", json!({ "path": "/comp/title" })),
            step("prim.rename", json!({ "path": "/comp/title", "name": "x" })),
            step("sublayer.set_edit_target", json!({ "id": "root" })),
            step("sublayer.create", json!({ "id": "anim/x" })),
            step(
                "sublayer.merge",
                json!({ "from": "agent/claude", "into": "root" }),
            ),
            step(
                "proposal.accept",
                json!({ "sublayer": "agent/claude", "into": "root" }),
            ),
        ];
        for d in denied {
            let err = s
                .execute(&mut ws, None, std::slice::from_ref(&d))
                .unwrap_err();
            assert!(err.is_denied(), "{}: {err}", d.command);
            assert_eq!(snapshot(&ws), before, "{}", d.command);
        }
        // A batch is all-or-nothing: the allowed first step is rolled back too.
        let err = s
            .execute(
                &mut ws,
                Some("batch"),
                &[
                    step(
                        "prim.create",
                        json!({ "path": "/comp/more", "prim_type": "Layer" }),
                    ),
                    step("prim.delete", json!({ "path": "/comp/title" })),
                ],
            )
            .unwrap_err();
        assert!(err.is_denied());
        assert_eq!(snapshot(&ws), before);
        // Deleting its own prim is fine.
        s.execute(
            &mut ws,
            None,
            &[step("prim.delete", json!({ "path": "/comp/mine" }))],
        )
        .unwrap();
        // Own sub-sublayers are allowed, and switching to them sticks.
        s.execute(
            &mut ws,
            None,
            &[
                step("sublayer.create", json!({ "id": "agent/claude/alt" })),
                step(
                    "sublayer.set_edit_target",
                    json!({ "id": "agent/claude/alt" }),
                ),
            ],
        )
        .unwrap();
        assert_eq!(s.edit_target().as_str(), "agent/claude/alt");
    }

    #[test]
    fn any_layer_may_accept_its_proposal() {
        let mut ws = workspace();
        let mut s = session(Permission::AnyLayer);
        s.execute(
            &mut ws,
            None,
            &[step(
                "prim.create",
                json!({ "path": "/comp/mine", "prim_type": "Layer" }),
            )],
        )
        .unwrap();
        s.execute(
            &mut ws,
            None,
            &[step(
                "proposal.accept",
                json!({ "sublayer": "agent/claude", "into": "root" }),
            )],
        )
        .unwrap();
        let root = ws.project.stage.sublayer(&SublayerId::root()).unwrap();
        assert!(root.has_spec(&PrimPath::new("/comp/mine").unwrap()));
    }

    #[test]
    fn failed_command_leaves_no_trace() {
        let mut ws = workspace();
        let before_stack = ws.project.stage.layer_stack().to_vec();
        let mut s = session(Permission::OwnLayer);
        let err = s
            .execute(
                &mut ws,
                None,
                &[step("prim.delete", json!({ "path": "/nope" }))],
            )
            .unwrap_err();
        assert!(matches!(err, AgentError::Command(_)), "{err}");
        // The proposal layer was created (lazily) but nothing written to it.
        let stage = &ws.project.stage;
        assert_eq!(stage.layer_stack().len(), before_stack.len() + 1);
        assert!(stage.sublayer(s.edit_target()).unwrap().is_empty());
    }
}
