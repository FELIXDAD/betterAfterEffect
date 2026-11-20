//! Agent workspace panels: the activity log (every tool call an agent made) and the
//! proposal list (what each agent's proposal sublayer changes, with accept / reject).
//!
//! Data comes from workstream 17 (`bae-agent`) through the [`AgentFeed`] trait, an
//! app resource (`Arc<dyn AgentFeed>`) the shell implements on top of
//! `bae_agent::AuditLog` and `bae_agent::proposal::proposals`, so `bae-ui` does not
//! depend on the agent crate. Accepting or rejecting runs the `proposal.accept` /
//! `proposal.reject` commands like any other change.

use std::sync::Arc;

use bae_model::Stage;
use egui::{RichText, Ui};

use crate::i18n::tr;
use crate::panel::{Panel, PanelContext};
use crate::theme::color;

/// Command ids of the proposal commands (`bae-agent`).
pub mod ids {
    pub const ACCEPT: &str = "proposal.accept";
    pub const REJECT: &str = "proposal.reject";
}

/// One agent tool call.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentActivity {
    /// Increasing.
    pub seq: u64,
    /// Start time, milliseconds since the Unix epoch.
    pub unix_ms: u64,
    /// Agent name (`claude-code`).
    pub agent: String,
    /// Tool id (`attr.set`, `stage.describe` …).
    pub tool: String,
    /// `None` when the call succeeded; the error or refusal otherwise.
    pub error: Option<String>,
    /// Changes the call made.
    pub changes: usize,
    pub duration_us: u64,
}

/// One prim a proposal changes.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposedPrim {
    pub path: String,
    /// The agent created it (rather than overriding an existing prim).
    pub created: bool,
    /// Attributes with proposed values.
    pub attrs: Vec<String>,
}

/// One proposal sublayer (`agent/<name>`).
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub sublayer: String,
    pub agent: String,
    pub prims: Vec<ProposedPrim>,
}

/// Where the agent panels get their data (an app resource, `Arc<dyn AgentFeed>`).
pub trait AgentFeed: Send + Sync + 'static {
    /// Tool calls with `seq > after`, oldest first.
    fn activity_since(&self, after: u64) -> Vec<AgentActivity>;
    /// Proposal sublayers in the stage, strongest first.
    fn proposals(&self, stage: &Stage) -> Vec<Proposal>;
}

fn feed(cx: &PanelContext<'_>) -> Option<Arc<dyn AgentFeed>> {
    cx.resources.get::<Arc<dyn AgentFeed>>().cloned()
}

/// "Agent Activity" (`"agent_activity"`).
#[derive(Default)]
pub struct ActivityPanel {
    entries: Vec<AgentActivity>,
    errors_only: bool,
}

/// Entries kept in the panel.
const MAX_ACTIVITY: usize = 2000;

impl Panel for ActivityPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let Some(feed) = feed(cx) else {
            empty(ui, tr("No agent is connected."));
            return;
        };
        let after = self.entries.last().map_or(0, |e| e.seq);
        self.entries.extend(feed.activity_since(after));
        if self.entries.len() > MAX_ACTIVITY {
            let drop = self.entries.len() - MAX_ACTIVITY;
            self.entries.drain(..drop);
        }
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                crate::widgets::checkbox(ui, &mut self.errors_only, tr("Only Errors"));
                ui.add_space(4.0);
                if self.entries.is_empty() {
                    ui.weak(tr("No agent activity yet."));
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for e in &self.entries {
                            if self.errors_only && e.error.is_none() {
                                continue;
                            }
                            activity_row(ui, e);
                        }
                    });
            });
    }
}

fn activity_row(ui: &mut Ui, e: &AgentActivity) {
    ui.horizontal(|ui| {
        let (mark, c) = match &e.error {
            None => ("✓", color::CACHE_GREEN),
            Some(_) => ("✗", color::TRAFFIC_CLOSE),
        };
        ui.label(RichText::new(mark).color(c));
        ui.label(
            RichText::new(clock(e.unix_ms))
                .size(10.5)
                .color(color::TEXT_DIM),
        );
        ui.label(RichText::new(&e.agent).color(color::ACCENT));
        ui.label(RichText::new(&e.tool).color(color::TEXT_STRONG));
        if e.changes > 0 {
            ui.label(
                RichText::new(format!("{} {}", e.changes, tr("changes")))
                    .size(10.5)
                    .color(color::TEXT_DIM),
            );
        }
    });
    if let Some(error) = &e.error {
        ui.label(RichText::new(error).size(10.5).color(color::TEXT_DIM));
    }
}

/// `HH:MM:SS` (UTC) of a Unix time in milliseconds.
fn clock(unix_ms: u64) -> String {
    let s = unix_ms / 1000;
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

/// "Proposals" (`"agent_proposals"`).
#[derive(Default)]
pub struct ProposalsPanel {
    expanded: Vec<String>,
}

impl Panel for ProposalsPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let Some(feed) = feed(cx) else {
            empty(ui, tr("No agent is connected."));
            return;
        };
        let proposals = feed.proposals(cx.stage);
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                if proposals.iter().all(|p| p.prims.is_empty()) {
                    ui.weak(tr("No proposals."));
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for p in proposals.iter().filter(|p| !p.prims.is_empty()) {
                            self.proposal(ui, cx, p);
                            ui.separator();
                        }
                    });
            });
    }
}

impl ProposalsPanel {
    fn proposal(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>, p: &Proposal) {
        let mut open = self.expanded.contains(&p.sublayer);
        ui.horizontal(|ui| {
            if crate::widgets::disclosure(ui, &mut open).changed() {
                if open {
                    self.expanded.push(p.sublayer.clone());
                } else {
                    self.expanded.retain(|s| *s != p.sublayer);
                }
            }
            ui.label(RichText::new(&p.agent).color(color::ACCENT));
            ui.label(
                RichText::new(format!("{} {}", p.prims.len(), tr("prims")))
                    .size(10.5)
                    .color(color::TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Reject All")).clicked() {
                    cx.invoke_with(ids::REJECT, serde_json::json!({ "sublayer": p.sublayer }));
                }
                if ui.button(tr("Accept All")).clicked() {
                    cx.invoke_with(ids::ACCEPT, serde_json::json!({ "sublayer": p.sublayer }));
                }
            });
        });
        if !open {
            return;
        }
        for prim in &p.prims {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                let badge = if prim.created {
                    tr("New")
                } else {
                    tr("Changed")
                };
                ui.label(RichText::new(badge).size(10.0).color(color::TEXT_DIM));
                ui.label(RichText::new(&prim.path).color(color::TEXT_STRONG));
                if !prim.attrs.is_empty() {
                    ui.label(
                        RichText::new(prim.attrs.join(", "))
                            .size(10.5)
                            .color(color::TEXT_DIM),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let params = serde_json::json!({ "sublayer": p.sublayer, "path": prim.path });
                    if ui.small_button(tr("Reject")).clicked() {
                        cx.invoke_with(ids::REJECT, params.clone());
                    }
                    if ui.small_button(tr("Accept")).clicked() {
                        cx.invoke_with(ids::ACCEPT, params);
                    }
                });
            });
        }
    }
}

fn empty(ui: &mut Ui, text: &str) {
    if ui.available_height() >= 40.0 {
        ui.centered_and_justified(|ui| {
            ui.weak(text);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::PanelId;
    use crate::panel::PanelRequests;
    use bae_app::Resources;
    use bae_model::AuthorId;
    use std::sync::Mutex;

    struct Fake {
        calls: Mutex<Vec<AgentActivity>>,
    }

    impl AgentFeed for Fake {
        fn activity_since(&self, after: u64) -> Vec<AgentActivity> {
            let calls = self.calls.lock().unwrap();
            calls.iter().filter(|c| c.seq > after).cloned().collect()
        }
        fn proposals(&self, _stage: &Stage) -> Vec<Proposal> {
            vec![Proposal {
                sublayer: "agent/claude".into(),
                agent: "claude".into(),
                prims: vec![ProposedPrim {
                    path: "/Comp1".into(),
                    created: true,
                    attrs: vec![],
                }],
            }]
        }
    }

    fn call(seq: u64) -> AgentActivity {
        AgentActivity {
            seq,
            unix_ms: 3_600_000 + seq * 1000,
            agent: "claude".into(),
            tool: "attr.set".into(),
            error: None,
            changes: 1,
            duration_us: 10,
        }
    }

    #[test]
    fn panels_read_the_feed_incrementally() {
        let fake = Arc::new(Fake {
            calls: Mutex::new(vec![call(1), call(2)]),
        });
        let mut resources = Resources::new();
        resources.insert::<Arc<dyn AgentFeed>>(fake.clone());
        let stage = Stage::new();
        let author = AuthorId::system();
        let id = PanelId::new("agent_activity");
        let mut requests = PanelRequests::default();
        let mut activity = ActivityPanel::default();
        let mut proposals = ProposalsPanel::default();
        let ctx = egui::Context::default();
        for round in 0..2 {
            if round == 1 {
                fake.calls.lock().unwrap().push(call(3));
            }
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut cx = PanelContext::new(&id, "", &stage, &author, &resources, &mut requests);
                activity.ui(ui, &mut cx);
                proposals.ui(ui, &mut cx);
            });
            out.textures_delta.clear();
        }
        let seqs: Vec<u64> = activity.entries.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [1, 2, 3], "no duplicates across frames");
        assert_eq!(clock(3_600_000 + 61_000), "01:01:01");
    }
}
