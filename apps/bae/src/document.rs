//! The open project and the command path into it.
//!
//! Everything that changes project data arrives here as a command id: undo/redo
//! go to `Stage::undo/redo` for the local author, every other id is run through
//! the `bae-model` command registry as one undo step (ARCHITECTURE §2 rule 1).

use std::sync::Arc;

use bae_model::{AuthorId, CommandError, CommandRegistry, Project, Stage};
use bae_ui::Invocation;
use bae_ui::action::ids;
use bae_ui::history::HistoryLog;

use crate::i18n::tr;

/// The open project (an untitled one until file commands land).
pub struct Document {
    project: Project,
    commands: CommandRegistry,
    author: AuthorId,
    /// Changed since the last save.
    dirty: bool,
    history: Option<Arc<HistoryLog>>,
}

/// What [`Document::run`] did with an invocation.
#[derive(Debug)]
pub enum Outcome {
    /// The project changed.
    Changed,
    /// Recognized but nothing to do (empty undo stack, …).
    Unchanged,
    /// No such command.
    Unknown,
    /// The command failed.
    Failed(CommandError),
}

impl Document {
    /// A new untitled project; changes are attributed to the local user. Every
    /// stage event is recorded in `history` (the History panel's data).
    pub fn new(history: Option<Arc<HistoryLog>>) -> Self {
        let mut project = Project::new();
        if let Some(log) = history.clone() {
            project.stage.subscribe(move |event| log.record(event));
        }
        Document {
            project,
            commands: CommandRegistry::with_builtins(),
            author: local_author(),
            dirty: false,
            history,
        }
    }

    /// `(id, English title)` of every model command (for the shortcut editor).
    pub fn command_titles(&self) -> Vec<(String, String)> {
        self.commands
            .infos()
            .map(|i| (i.id.clone(), i.title.clone()))
            .collect()
    }

    pub fn stage(&self) -> &Stage {
        &self.project.stage
    }

    pub fn author(&self) -> &AuthorId {
        &self.author
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Window title: project name (AE: "Untitled Project.aep").
    pub fn title(&self) -> String {
        let name = self
            .project
            .location()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr("Untitled Project").to_owned());
        format!("{} - {name}", bae_platform::APP_NAME)
    }

    /// Labels for the Edit menu: `(can_undo, undo label, can_redo, redo label)`.
    pub fn undo_state(&self) -> (Option<String>, Option<String>) {
        let stage = &self.project.stage;
        let undo = stage.can_undo(&self.author).then(|| {
            stage
                .undo_label(&self.author)
                .unwrap_or_default()
                .to_owned()
        });
        let redo = stage.can_redo(&self.author).then(|| {
            stage
                .redo_label(&self.author)
                .unwrap_or_default()
                .to_owned()
        });
        (undo, redo)
    }

    /// Run a command id: undo/redo or a registered model command.
    pub fn run(&mut self, inv: &Invocation) -> Outcome {
        let stage = &mut self.project.stage;
        let label = |text: Option<&str>| {
            if let (Some(log), Some(text)) = (&self.history, text) {
                log.label_next(text);
            }
        };
        let changed = match inv.id.as_str() {
            ids::UNDO => {
                label(stage.undo_label(&self.author));
                stage.undo(&self.author).is_some()
            }
            ids::REDO => {
                label(stage.redo_label(&self.author));
                stage.redo(&self.author).is_some()
            }
            id => {
                let Some(cmd) = self.commands.get(id) else {
                    return Outcome::Unknown;
                };
                label(Some(&cmd.info().title));
                let params = if inv.params.is_null() {
                    serde_json::json!({})
                } else {
                    inv.params.clone()
                };
                match self.commands.execute(stage, &self.author, id, params) {
                    Ok(out) => !out.event.is_empty(),
                    Err(error) => return Outcome::Failed(error),
                }
            }
        };
        if changed {
            self.dirty = true;
            Outcome::Changed
        } else {
            Outcome::Unchanged
        }
    }
}

/// `user:<login>` from the environment, `user:local` if unknown.
fn local_author() -> AuthorId {
    let login = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    AuthorId::new(&format!("user:{login}"))
        .ok()
        .filter(|_| !login.is_empty())
        .unwrap_or_else(|| AuthorId::new("user:local").unwrap_or_else(|_| AuthorId::system()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_undo_go_through_the_model() {
        let log = Arc::new(HistoryLog::new());
        let mut doc = Document::new(Some(log.clone()));
        assert!(matches!(
            doc.run(&Invocation::new("no.such.command")),
            Outcome::Unknown
        ));
        assert!(matches!(
            doc.run(&Invocation::new(ids::UNDO)),
            Outcome::Unchanged
        ));
        let create = Invocation::with_params(
            "prim.create",
            serde_json::json!({ "path": "/Comp1", "prim_type": "Comp" }),
        );
        match doc.run(&create) {
            Outcome::Changed => {}
            other => panic!("prim.create: {other:?}"),
        }
        assert!(doc.is_dirty());
        let (undo, redo) = doc.undo_state();
        assert!(undo.is_some() && redo.is_none());
        assert!(matches!(
            doc.run(&Invocation::new(ids::UNDO)),
            Outcome::Changed
        ));
        let (undo, redo) = doc.undo_state();
        assert!(undo.is_none() && redo.is_some());
        let labels: Vec<_> = log.entries().into_iter().map(|e| e.label).collect();
        assert_eq!(labels, ["Create Prim", "Create Prim"]);
        assert_eq!(log.entries()[1].origin, bae_model::EventOrigin::Undo);
    }
}
