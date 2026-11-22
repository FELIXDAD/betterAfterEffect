//! History: the list of committed steps with their authors (users, collaborators,
//! agents), shown by the History panel (AE: Edit > History).
//!
//! The shell owns an `Arc<HistoryLog>` app resource and feeds it from
//! `Stage::subscribe` ([`HistoryLog::record`]); command labels come from
//! [`HistoryLog::label_next`], called just before a command runs (the stage event
//! itself carries no label yet — see the request to 02 in its task book).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::SystemTime;

use bae_model::{AuthorId, EventOrigin, StageEvent};
use egui::{Color32, RichText, Ui};

use crate::action::ids;
use crate::i18n::tr;
use crate::panel::{Panel, PanelContext};
use crate::theme::color;

/// One committed step.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    /// Sequence number (monotonic, starting at 1).
    pub seq: u64,
    /// Command title (English) or a description of the change.
    pub label: String,
    /// `None` for changes imported from collaborators without author info.
    pub author: Option<AuthorId>,
    pub origin: EventOrigin,
    /// Number of changed prims / attributes.
    pub changes: usize,
    pub time: SystemTime,
}

/// A bounded, thread-safe log of stage events (an app resource, `Arc<HistoryLog>`).
#[derive(Debug)]
pub struct HistoryLog {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: VecDeque<HistoryEntry>,
    next_label: Option<String>,
    seq: u64,
}

impl Default for HistoryLog {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryLog {
    /// Oldest entries are dropped beyond this many.
    pub const CAPACITY: usize = 1000;

    pub fn new() -> Self {
        HistoryLog {
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Label for the next recorded event (the command about to run).
    pub fn label_next(&self, label: impl Into<String>) {
        self.lock().next_label = Some(label.into());
    }

    /// Record a stage event (call from `Stage::subscribe`). Empty events and layer
    /// stack changes without changes are skipped.
    pub fn record(&self, event: &StageEvent) {
        let mut inner = self.lock();
        let label = inner.next_label.take();
        if event.is_empty() {
            return;
        }
        let label = label.unwrap_or_else(|| describe(event));
        inner.seq += 1;
        let entry = HistoryEntry {
            seq: inner.seq,
            label,
            author: event.author.clone(),
            origin: event.origin,
            changes: event.changes.len(),
            time: SystemTime::now(),
        };
        inner.entries.push_back(entry);
        while inner.entries.len() > Self::CAPACITY {
            inner.entries.pop_front();
        }
    }

    /// All entries, oldest first.
    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.lock().entries.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sequence number of the newest entry (0 when empty); cheap change check.
    pub fn last_seq(&self) -> u64 {
        self.lock().seq
    }

    /// Forget every entry (another project was opened). The sequence number still
    /// advances, so readers that compare [`HistoryLog::last_seq`] see a change.
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.entries.clear();
        inner.next_label = None;
        inner.seq += 1;
    }
}

/// A label for events without a command title (remote imports, layer stack).
fn describe(event: &StageEvent) -> String {
    let what = match event.origin {
        EventOrigin::Remote => "Remote change",
        EventOrigin::Stack => "Layer stack change",
        EventOrigin::Undo => "Undo",
        EventOrigin::Redo => "Redo",
        EventOrigin::Local => "Change",
    };
    // Translated here (the language is fixed at startup): the panel shows labels
    // through `tr`, which cannot translate a word followed by a path.
    let what = crate::i18n::tr(what);
    match event.changes.first() {
        Some(c) => format!("{what} {}", c.path()),
        None => what.to_owned(),
    }
}

/// How an author is shown: "You", "Agent claude-01", or the user name.
pub fn author_label(author: Option<&AuthorId>, me: &AuthorId) -> (String, Color32) {
    match author {
        None => (tr("Collaborator").to_owned(), color::TEXT_DIM),
        Some(a) if a == me => (tr("You").to_owned(), color::TEXT),
        Some(a) if a.is_agent() => (
            format!(
                "{} {}",
                tr("Agent"),
                a.as_str().trim_start_matches("agent:")
            ),
            color::ACCENT,
        ),
        Some(a) => (
            a.as_str().trim_start_matches("user:").to_owned(),
            color::TEXT_STRONG,
        ),
    }
}

/// The History panel (`"history"`).
#[derive(Default)]
pub struct HistoryPanel {
    /// Show only the local user's steps.
    mine_only: bool,
    last_seq: u64,
}

impl Panel for HistoryPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let Some(log) = cx.resources.get::<std::sync::Arc<HistoryLog>>() else {
            ui.weak(tr("No history."));
            return;
        };
        let entries = log.entries();
        let stage = cx.stage;
        let me = cx.author.clone();
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let undo = stage.can_undo(&me);
                    let redo = stage.can_redo(&me);
                    if ui
                        .add_enabled(undo, egui::Button::new(tr("Undo")))
                        .clicked()
                    {
                        cx.invoke(ids::UNDO);
                    }
                    if ui
                        .add_enabled(redo, egui::Button::new(tr("Redo")))
                        .clicked()
                    {
                        cx.invoke(ids::REDO);
                    }
                    ui.add_space(8.0);
                    crate::widgets::checkbox(ui, &mut self.mine_only, tr("Only My Changes"));
                });
                ui.add_space(4.0);
                if entries.is_empty() {
                    ui.weak(tr("No history."));
                    return;
                }
                let stick = log.last_seq() != self.last_seq;
                self.last_seq = log.last_seq();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(stick)
                    .show(ui, |ui| {
                        for e in &entries {
                            if self.mine_only && e.author.as_ref() != Some(&me) {
                                continue;
                            }
                            entry_row(ui, e, &me);
                        }
                    });
            });
    }

    fn menu(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        crate::widgets::checkbox(ui, &mut self.mine_only, tr("Only My Changes"));
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "mine_only": self.mine_only }))
    }
}

impl HistoryPanel {
    pub fn from_state(state: Option<&serde_json::Value>) -> Self {
        HistoryPanel {
            mine_only: state
                .and_then(|s| s.get("mine_only"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            last_seq: 0,
        }
    }
}

fn entry_row(ui: &mut Ui, e: &HistoryEntry, me: &AuthorId) {
    let (who, who_color) = author_label(e.author.as_ref(), me);
    ui.horizontal(|ui| {
        let prefix = match e.origin {
            EventOrigin::Undo => format!("{} ", tr("Undo")),
            EventOrigin::Redo => format!("{} ", tr("Redo")),
            _ => String::new(),
        };
        ui.label(RichText::new(format!("{prefix}{}", tr(&e.label))).color(color::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(who).size(10.5).color(who_color));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::{CommandRegistry, Stage};

    #[test]
    fn records_labels_authors_and_origins() {
        let log = std::sync::Arc::new(HistoryLog::new());
        let mut stage = Stage::new();
        {
            let log = log.clone();
            stage.subscribe(move |e| log.record(e));
        }
        let registry = CommandRegistry::with_builtins();
        let agent = AuthorId::new("agent:claude-01").unwrap();
        let me = AuthorId::new("user:me").unwrap();
        log.label_next("New Composition");
        registry
            .execute(
                &mut stage,
                &me,
                "prim.create",
                serde_json::json!({ "path": "/Comp1", "prim_type": "Comp" }),
            )
            .unwrap();
        registry
            .execute(
                &mut stage,
                &agent,
                "prim.create",
                serde_json::json!({ "path": "/Comp2", "prim_type": "Comp" }),
            )
            .unwrap();
        stage.undo(&me);
        let entries = log.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].label, "New Composition");
        assert_eq!(entries[0].author.as_ref(), Some(&me));
        assert_eq!(entries[1].author.as_ref(), Some(&agent));
        assert!(entries[1].label.contains("/Comp2"), "{}", entries[1].label);
        assert_eq!(entries[2].origin, EventOrigin::Undo);
        assert_eq!(author_label(Some(&agent), &me).0, "Agent claude-01");
        assert_eq!(author_label(Some(&me), &me).0, "You");
        // Opening another project clears the log; the sequence still advances.
        let seq = log.last_seq();
        log.label_next("dangling");
        log.clear();
        assert!(log.is_empty());
        assert!(log.last_seq() > seq);
        stage.undo(&agent);
        assert_ne!(log.entries()[0].label, "dangling");
    }
}
