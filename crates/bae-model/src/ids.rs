//! Identifiers for sublayers and authors.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! string_id {
    ($(#[$m:meta])* $name:ident, $validate:expr, $desc:literal) => {
        $(#[$m])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            /// Validate and create.
            pub fn new(s: &str) -> Result<Self, InvalidId> {
                let valid: fn(&str) -> bool = $validate;
                if valid(s) {
                    Ok($name(Arc::from(s)))
                } else {
                    Err(InvalidId { kind: stringify!($name), value: s.to_owned() })
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::str::FromStr for $name {
            type Err = InvalidId;
            fn from_str(s: &str) -> Result<Self, InvalidId> {
                $name::new(s)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = Cow::<str>::deserialize(d)?;
                $name::new(&s).map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "description": $desc })
            }
        }
    };
}

/// Error for a malformed [`SublayerId`] or [`AuthorId`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind}: {value:?}")]
pub struct InvalidId {
    pub kind: &'static str,
    pub value: String,
}

fn valid_sublayer(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('/')
        && !s.ends_with('/')
        && !s.contains("//")
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
}

fn valid_author(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

string_id!(
    /// Sublayer ("工程层") ID, unique within a project, e.g. `root`, `anim/shot010`,
    /// `agent/claude-01`. Letters, digits, `_`, `-` and `/` as a separator.
    SublayerId,
    valid_sublayer,
    "Sublayer ID, e.g. root, anim/shot010, agent/claude-01"
);

string_id!(
    /// Who made a change: a user or an agent, e.g. `user:felix`, `agent:claude-01`.
    /// Undo and redo are per author.
    AuthorId,
    valid_author,
    "Author ID, e.g. user:alice or agent:claude-01"
);

impl SublayerId {
    /// The weakest sublayer every project has (project settings).
    pub fn root() -> Self {
        SublayerId(Arc::from("root"))
    }
}

impl AuthorId {
    /// Author used by tests, importers and migrations that act on behalf of nobody.
    pub fn system() -> Self {
        AuthorId(Arc::from("system"))
    }

    /// True for agent authors (`agent:` prefix).
    pub fn is_agent(&self) -> bool {
        self.0.starts_with("agent:")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(SublayerId::new("anim/shot010").is_ok());
        for bad in ["", "/a", "a/", "a//b", "a b", "a.b"] {
            assert!(SublayerId::new(bad).is_err(), "{bad:?}");
        }
        assert!(AuthorId::new("agent:claude-01").unwrap().is_agent());
        assert!(AuthorId::new("").is_err());
    }
}
