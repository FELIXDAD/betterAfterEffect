//! Feeds `bae-ui`'s agent panels from `bae-agent`: the audit log of tool calls and
//! the proposal sublayers of the open stage.
//!
//! The shell inserts one `Arc<AuditLog>` resource; the in-app MCP server (workstream
//! 17, HTTP transport) is expected to record into the same log.

use std::sync::Arc;

use bae_agent::{AuditLog, AuditOutcome};
use bae_model::Stage;
use bae_ui::agent::{AgentActivity, AgentFeed, Proposal, ProposedPrim};

/// `AgentFeed` over `bae-agent`.
pub struct BaeAgentFeed {
    audit: Arc<AuditLog>,
}

impl BaeAgentFeed {
    pub fn new(audit: Arc<AuditLog>) -> Self {
        BaeAgentFeed { audit }
    }
}

impl AgentFeed for BaeAgentFeed {
    fn activity_since(&self, after: u64) -> Vec<AgentActivity> {
        self.audit
            .entries_since(after)
            .into_iter()
            .map(|e| AgentActivity {
                seq: e.seq,
                unix_ms: e.unix_ms,
                agent: e.agent,
                tool: e.tool,
                error: match e.outcome {
                    AuditOutcome::Ok => None,
                    AuditOutcome::Error { message } | AuditOutcome::Denied { message } => {
                        Some(message)
                    }
                },
                changes: e.changes,
                duration_us: e.duration_us,
            })
            .collect()
    }

    fn proposals(&self, stage: &Stage) -> Vec<Proposal> {
        bae_agent::proposal::proposals(stage)
            .into_iter()
            .map(|p| Proposal {
                sublayer: p.sublayer.to_string(),
                agent: p.agent,
                prims: p
                    .prims
                    .into_iter()
                    .map(|prim| ProposedPrim {
                        path: prim.path.to_string(),
                        created: prim.created,
                        attrs: prim.attrs,
                    })
                    .collect(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::{AuthorId, CommandRegistry, SublayerId};

    #[test]
    fn proposals_and_activity_are_mapped() {
        let mut stage = Stage::new();
        let mut registry = CommandRegistry::with_builtins();
        bae_agent::register_commands(&mut registry);
        let agent = AuthorId::new("agent:claude").unwrap();
        let layer = SublayerId::new("agent/claude").unwrap();
        stage.create_sublayer(layer.clone(), 0).unwrap();
        stage.set_edit_target(&layer).unwrap();
        registry
            .execute(
                &mut stage,
                &agent,
                "prim.create",
                serde_json::json!({ "path": "/Comp1", "prim_type": "Comp" }),
            )
            .unwrap();
        let feed = BaeAgentFeed::new(Arc::new(AuditLog::new()));
        let proposals = feed.proposals(&stage);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].agent, "claude");
        assert_eq!(proposals[0].prims[0].path, "/Comp1");
        assert!(proposals[0].prims[0].created);
        assert!(feed.activity_since(0).is_empty());
    }
}
