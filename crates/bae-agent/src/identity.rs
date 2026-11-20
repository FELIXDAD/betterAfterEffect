//! Agent identity and permission levels.

use std::fmt;
use std::str::FromStr;

use bae_model::{AuthorId, SublayerId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an agent may write (ARCHITECTURE §10: three levels).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// Queries and renders only; every command is refused.
    ReadOnly,
    /// Commands may only write the agent's own sublayers (`agent/<name>` and below).
    #[default]
    OwnLayer,
    /// Commands may write any sublayer. Edits still default to the proposal layer.
    AnyLayer,
}

impl Permission {
    pub const ALL: [Permission; 3] = [
        Permission::ReadOnly,
        Permission::OwnLayer,
        Permission::AnyLayer,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Permission::ReadOnly => "read-only",
            Permission::OwnLayer => "own-layer",
            Permission::AnyLayer => "any-layer",
        }
    }

    /// Whether commands may run at all.
    pub fn can_write(self) -> bool {
        self != Permission::ReadOnly
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Permission {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace('_', "-").as_str() {
            "read-only" | "readonly" | "ro" => Ok(Permission::ReadOnly),
            "own-layer" | "own" => Ok(Permission::OwnLayer),
            "any-layer" | "any" => Ok(Permission::AnyLayer),
            _ => Err(format!(
                "unknown permission {s:?} (expected read-only, own-layer or any-layer)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid agent name {0:?}: use 1-64 ASCII letters, digits, '_' or '-', not starting with '-'"
)]
pub struct InvalidAgentName(pub String);

/// An agent collaborator: its name, author ID (`agent:<name>`, used for undo and
/// history) and proposal sublayer (`agent/<name>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentIdentity {
    name: String,
    author: AuthorId,
    proposal_layer: SublayerId,
}

const MAX_NAME: usize = 64;

impl AgentIdentity {
    pub fn new(name: &str) -> Result<Self, InvalidAgentName> {
        let valid = !name.is_empty()
            && name.len() <= MAX_NAME
            && !name.starts_with('-')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid {
            return Err(InvalidAgentName(name.to_owned()));
        }
        let bad = || InvalidAgentName(name.to_owned());
        Ok(AgentIdentity {
            name: name.to_owned(),
            author: AuthorId::new(&format!("agent:{name}")).map_err(|_| bad())?,
            proposal_layer: SublayerId::new(&format!("agent/{name}")).map_err(|_| bad())?,
        })
    }

    /// Derive an identity from a free-form client name (e.g. MCP `clientInfo.name`
    /// "Claude Code" → `claude-code`). Falls back to `agent`.
    pub fn from_client_name(raw: &str) -> Self {
        let mut name = String::new();
        for c in raw.trim().chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() || c == '_' {
                name.push(c);
            } else if !name.is_empty() && !name.ends_with('-') {
                name.push('-');
            }
            if name.len() >= MAX_NAME {
                break;
            }
        }
        let name = name.trim_matches('-');
        AgentIdentity::new(name).unwrap_or_else(|_| AgentIdentity::new("agent").expect("valid"))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn author(&self) -> &AuthorId {
        &self.author
    }

    /// The sublayer this agent writes to by default.
    pub fn proposal_layer(&self) -> &SublayerId {
        &self.proposal_layer
    }

    /// Whether `id` is one of this agent's own sublayers (`agent/<name>` or below).
    pub fn owns(&self, id: &SublayerId) -> bool {
        let own = self.proposal_layer.as_str();
        id.as_str() == own
            || id
                .as_str()
                .strip_prefix(own)
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let a = AgentIdentity::new("claude-01").unwrap();
        assert_eq!(a.author().as_str(), "agent:claude-01");
        assert_eq!(a.proposal_layer().as_str(), "agent/claude-01");
        for bad in ["", "-x", "a b", "a/b", "a.b", &"x".repeat(65)] {
            assert!(AgentIdentity::new(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            AgentIdentity::from_client_name("Claude Code").name(),
            "claude-code"
        );
        assert_eq!(
            AgentIdentity::from_client_name("  codex-mcp-client ").name(),
            "codex-mcp-client"
        );
        assert_eq!(AgentIdentity::from_client_name("你好").name(), "agent");
    }

    #[test]
    fn ownership() {
        let a = AgentIdentity::new("claude").unwrap();
        let id = |s: &str| SublayerId::new(s).unwrap();
        assert!(a.owns(&id("agent/claude")));
        assert!(a.owns(&id("agent/claude/shot010")));
        assert!(!a.owns(&id("agent/claude2")));
        assert!(!a.owns(&id("agent/codex")));
        assert!(!a.owns(&id("root")));
    }

    #[test]
    fn permission_parse() {
        for p in Permission::ALL {
            assert_eq!(p.as_str().parse::<Permission>().unwrap(), p);
        }
        assert_eq!("read_only".parse::<Permission>(), Ok(Permission::ReadOnly));
        assert!("admin".parse::<Permission>().is_err());
    }
}
