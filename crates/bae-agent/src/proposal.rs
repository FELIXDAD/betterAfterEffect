//! Reviewing agent proposals (ARCHITECTURE §4.3, §10).
//!
//! An agent writes into its proposal sublayer `agent/<name>`. The user (Agent panel,
//! workstream 11) accepts or rejects the whole layer, one prim, or one attribute:
//!
//! - `proposal.accept` moves the opinions into a target sublayer (default `root`).
//! - `proposal.reject` deletes them from the proposal layer.
//!
//! Both are ordinary undoable commands. [`proposals`] lists what each agent layer
//! changes, for the review UI.

use bae_model::commands::move_opinions;
use bae_model::{
    Command, CommandError, CommandRegistry, ModelError, Op, PrimPath, Specifier, Stage, SublayerId,
    Transaction,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Register `proposal.accept` and `proposal.reject`.
pub fn register(r: &mut CommandRegistry) {
    for res in [r.register(AcceptProposal), r.register(RejectProposal)] {
        res.expect("proposal command IDs are unique");
    }
}

/// Whether a sublayer is an agent proposal layer (`agent/…`).
pub fn is_proposal_layer(id: &SublayerId) -> bool {
    id.as_str().starts_with("agent/")
}

fn proposal_layer<'s>(
    stage: &'s Stage,
    id: &SublayerId,
) -> Result<&'s bae_model::Sublayer, CommandError> {
    if !is_proposal_layer(id) {
        return Err(CommandError::Failed(format!(
            "{id} is not an agent proposal sublayer (agent/<name>)"
        )));
    }
    stage
        .sublayer(id)
        .ok_or_else(|| ModelError::UnknownSublayer(id.clone()).into())
}

/// Paths in `layer` at or below `under` (all paths when `None`), parents first.
fn paths_under(layer: &bae_model::Sublayer, under: Option<&PrimPath>) -> Vec<PrimPath> {
    let mut paths: Vec<PrimPath> = layer
        .paths()
        .filter(|p| under.is_none_or(|u| u.is_prefix_of(p)))
        .cloned()
        .collect();
    paths.sort_by_key(|p| p.depth());
    paths
}

pub struct AcceptProposal;

#[derive(Deserialize, JsonSchema)]
pub struct AcceptProposalParams {
    /// Proposal sublayer, e.g. agent/claude-code.
    pub sublayer: SublayerId,
    /// Sublayer the accepted opinions are written into. Default: root.
    #[serde(default)]
    pub into: Option<SublayerId>,
    /// Accept only this prim and its descendants. Omitted accepts everything.
    #[serde(default)]
    pub path: Option<PrimPath>,
    /// With `path`: accept only this attribute of that prim.
    #[serde(default)]
    pub attr: Option<String>,
}

/// Output of the proposal commands.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProposalOutput {
    /// Prims whose opinions were accepted or rejected.
    pub prims: Vec<PrimPath>,
}

impl Command for AcceptProposal {
    type Params = AcceptProposalParams;
    type Output = ProposalOutput;
    const ID: &'static str = "proposal.accept";
    const TITLE: &'static str = "Accept Proposal";
    const DESCRIPTION: &'static str = "Accept an agent's proposed changes: move them from its proposal sublayer (agent/<name>) into another sublayer (default root). Accepts everything, one prim subtree, or one attribute.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ProposalOutput, CommandError> {
        let into = p.into.unwrap_or_else(SublayerId::root);
        if into == p.sublayer {
            return Err(CommandError::Failed(
                "cannot accept a proposal into itself".into(),
            ));
        }
        tx.stage()
            .sublayer(&into)
            .ok_or_else(|| ModelError::UnknownSublayer(into.clone()))?;
        let layer = proposal_layer(tx.stage(), &p.sublayer)?;
        if p.attr.is_some() && p.path.is_none() {
            return Err(CommandError::Failed("`attr` needs `path`".into()));
        }
        if let (Some(path), Some(attr)) = (&p.path, &p.attr) {
            if layer.attr(path, attr).is_none() {
                return Err(CommandError::Failed(format!(
                    "{} has no proposed value for {path} {attr}",
                    p.sublayer
                )));
            }
            move_opinions(tx, path, Some(attr), &p.sublayer, &into, true)?;
            return Ok(ProposalOutput {
                prims: vec![path.clone()],
            });
        }
        let paths = paths_under(layer, p.path.as_ref());
        if paths.is_empty() {
            return Err(CommandError::Failed(format!(
                "{} proposes nothing there",
                p.sublayer
            )));
        }
        for path in &paths {
            // An earlier move may have pruned now-empty specs below it.
            if tx
                .stage()
                .sublayer(&p.sublayer)
                .is_some_and(|l| l.has_spec(path))
            {
                move_opinions(tx, path, None, &p.sublayer, &into, true)?;
            }
        }
        Ok(ProposalOutput { prims: paths })
    }
}

pub struct RejectProposal;

#[derive(Deserialize, JsonSchema)]
pub struct RejectProposalParams {
    /// Proposal sublayer, e.g. agent/claude-code.
    pub sublayer: SublayerId,
    /// Reject only this prim and its descendants. Omitted rejects everything.
    #[serde(default)]
    pub path: Option<PrimPath>,
    /// With `path`: reject only this attribute of that prim.
    #[serde(default)]
    pub attr: Option<String>,
}

impl Command for RejectProposal {
    type Params = RejectProposalParams;
    type Output = ProposalOutput;
    const ID: &'static str = "proposal.reject";
    const TITLE: &'static str = "Reject Proposal";
    const DESCRIPTION: &'static str = "Reject an agent's proposed changes: delete them from its proposal sublayer (agent/<name>). Rejects everything, one prim subtree, or one attribute.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<ProposalOutput, CommandError> {
        let layer = proposal_layer(tx.stage(), &p.sublayer)?;
        match (&p.path, &p.attr) {
            (None, Some(_)) => Err(CommandError::Failed("`attr` needs `path`".into())),
            (Some(path), Some(attr)) => {
                if layer.attr(path, attr).is_none() {
                    return Err(CommandError::Failed(format!(
                        "{} has no proposed value for {path} {attr}",
                        p.sublayer
                    )));
                }
                tx.apply_to(
                    &p.sublayer,
                    Op::SetAttr {
                        path: path.clone(),
                        attr: attr.clone(),
                        spec: None,
                    },
                )?;
                Ok(ProposalOutput {
                    prims: vec![path.clone()],
                })
            }
            (Some(path), None) => {
                if !layer.has_spec(path) {
                    return Err(CommandError::Failed(format!(
                        "{} proposes nothing at {path}",
                        p.sublayer
                    )));
                }
                let prims = paths_under(layer, Some(path));
                tx.apply_to(&p.sublayer, Op::DeletePrim { path: path.clone() })?;
                Ok(ProposalOutput { prims })
            }
            (None, None) => {
                let prims = paths_under(layer, None);
                for top in layer.children(&PrimPath::root()) {
                    tx.apply_to(&p.sublayer, Op::DeletePrim { path: top })?;
                }
                Ok(ProposalOutput { prims })
            }
        }
    }
}

/// One prim an agent layer has opinions about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProposedPrim {
    pub path: PrimPath,
    /// The agent created the prim (`def`) rather than overriding an existing one.
    pub created: bool,
    /// Attributes with proposed opinions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    /// Type or references changed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub structure: bool,
}

/// What one proposal sublayer changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProposalSummary {
    pub sublayer: SublayerId,
    /// Agent name (`agent/<name>` → `<name>`, nested layers keep the rest).
    pub agent: String,
    /// Prims with opinions, parents first. Pure `over` specs that only hold children
    /// are left out.
    pub prims: Vec<ProposedPrim>,
}

impl ProposalSummary {
    pub fn is_empty(&self) -> bool {
        self.prims.is_empty()
    }
}

/// Summarize one proposal sublayer.
pub fn summarize(stage: &Stage, id: &SublayerId) -> Option<ProposalSummary> {
    let layer = stage.sublayer(id)?;
    let prims = paths_under(layer, None)
        .into_iter()
        .filter_map(|path| {
            let spec = layer.spec(&path)?;
            let created = spec.specifier == Specifier::Def;
            let structure = spec.prim_type.is_some() || !spec.references.is_empty();
            let attrs: Vec<String> = spec.attrs.keys().cloned().collect();
            (created || structure || !attrs.is_empty()).then_some(ProposedPrim {
                path,
                created,
                attrs,
                structure,
            })
        })
        .collect();
    Some(ProposalSummary {
        sublayer: id.clone(),
        agent: id.as_str().trim_start_matches("agent/").to_owned(),
        prims,
    })
}

/// Every proposal sublayer in the stack, strongest first.
pub fn proposals(stage: &Stage) -> Vec<ProposalSummary> {
    stage
        .layer_stack()
        .iter()
        .filter(|id| is_proposal_layer(id))
        .filter_map(|id| summarize(stage, id))
        .collect()
}

#[cfg(test)]
mod tests {
    use bae_model::{AuthorId, Value};
    use serde_json::json;

    use super::*;
    use crate::Workspace;

    fn setup() -> (Workspace, AuthorId) {
        let mut ws = Workspace::new(bae_model::Project::new());
        let user = AuthorId::new("user:felix").unwrap();
        let stage = &mut ws.project.stage;
        let reg = &ws.registry;
        reg.execute(
            stage,
            &user,
            "prim.create",
            json!({"path": "/comp", "prim_type": "Comp"}),
        )
        .unwrap();
        reg.execute(
            stage,
            &user,
            "sublayer.create",
            json!({"id": "agent/claude"}),
        )
        .unwrap();
        reg.execute(
            stage,
            &user,
            "sublayer.set_edit_target",
            json!({"id": "agent/claude"}),
        )
        .unwrap();
        reg.execute(
            stage,
            &user,
            "prim.create",
            json!({"path": "/comp/title", "prim_type": "Layer"}),
        )
        .unwrap();
        for (attr, v) in [("opacity", 50.0), ("rotation", 10.0)] {
            reg.execute(
                stage,
                &user,
                "attr.set",
                json!({"path": "/comp", "attr": attr, "value": {"type": "float", "value": v}}),
            )
            .unwrap();
        }
        stage.set_edit_target(&SublayerId::root()).unwrap();
        (ws, user)
    }

    fn agent() -> SublayerId {
        SublayerId::new("agent/claude").unwrap()
    }

    #[test]
    fn summary_lists_created_and_overridden_prims() {
        let (ws, _) = setup();
        let all = proposals(&ws.project.stage);
        assert_eq!(all.len(), 1);
        let s = &all[0];
        assert_eq!(s.agent, "claude");
        let comp = &s.prims[0];
        assert_eq!(comp.path.as_str(), "/comp");
        assert!(!comp.created);
        assert_eq!(comp.attrs, vec!["opacity", "rotation"]);
        assert!(s.prims[1].created);
    }

    #[test]
    fn accept_one_attribute_then_reject_the_rest() {
        let (mut ws, user) = setup();
        let stage = &mut ws.project.stage;
        let reg = &ws.registry;
        reg.execute(
            stage,
            &user,
            "proposal.accept",
            json!({"sublayer": "agent/claude", "path": "/comp", "attr": "opacity"}),
        )
        .unwrap();
        let comp = PrimPath::new("/comp").unwrap();
        assert_eq!(
            stage.prim(&comp).unwrap().attr_source("opacity"),
            Some(&SublayerId::root())
        );
        assert_eq!(
            stage.prim(&comp).unwrap().value("opacity"),
            Some(&Value::Float(50.0))
        );
        reg.execute(
            stage,
            &user,
            "proposal.reject",
            json!({"sublayer": "agent/claude"}),
        )
        .unwrap();
        assert!(stage.sublayer(&agent()).unwrap().is_empty());
        assert!(stage.prim(&comp).unwrap().attr("rotation").is_none());
        assert!(!stage.has_prim(&PrimPath::new("/comp/title").unwrap()));
        // Undo brings the proposal back.
        stage.undo(&user).unwrap();
        assert!(stage.has_prim(&PrimPath::new("/comp/title").unwrap()));
    }

    #[test]
    fn accept_everything() {
        let (mut ws, user) = setup();
        let stage = &mut ws.project.stage;
        ws.registry
            .execute(
                stage,
                &user,
                "proposal.accept",
                json!({"sublayer": "agent/claude"}),
            )
            .unwrap();
        assert!(proposals(stage)[0].is_empty());
        let root = stage.sublayer(&SublayerId::root()).unwrap();
        assert!(root.has_spec(&PrimPath::new("/comp/title").unwrap()));
        assert!(
            root.attr(&PrimPath::new("/comp").unwrap(), "rotation")
                .is_some()
        );
    }

    #[test]
    fn rejects_non_proposal_layers() {
        let (mut ws, user) = setup();
        let err = ws
            .registry
            .execute(
                &mut ws.project.stage,
                &user,
                "proposal.reject",
                json!({"sublayer": "root"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("not an agent proposal"), "{err}");
    }
}
