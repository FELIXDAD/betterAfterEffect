//! Change notifications (ARCHITECTURE §4.5). Render caches invalidate from these.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{AuthorId, SublayerId};
use crate::path::PrimPath;
use crate::time::TimeRange;

/// What changed. Paths are composed-stage paths. A change in a weak sublayer may be
/// hidden by a stronger opinion; events are conservative and still report it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    PrimCreated {
        path: PrimPath,
    },
    /// The prim and all its descendants are gone (from that sublayer).
    PrimRemoved {
        path: PrimPath,
    },
    /// The prim (and its subtree) moved or was renamed, or was reordered (`from == to`).
    PrimMoved {
        from: PrimPath,
        to: PrimPath,
    },
    /// Type, specifier or references changed.
    PrimChanged {
        path: PrimPath,
    },
    /// An attribute's opinion changed; values in `range` may differ.
    AttrChanged {
        path: PrimPath,
        attr: String,
        range: TimeRange,
    },
    /// Anything under `path` may have changed (layer stack edits, bulk imports).
    Resynced {
        path: PrimPath,
    },
}

impl Change {
    /// The primary path affected (for moves, the destination).
    pub fn path(&self) -> &PrimPath {
        match self {
            Change::PrimCreated { path }
            | Change::PrimRemoved { path }
            | Change::PrimChanged { path }
            | Change::AttrChanged { path, .. }
            | Change::Resynced { path } => path,
            Change::PrimMoved { to, .. } => to,
        }
    }
}

/// Why an event happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventOrigin {
    /// A local transaction (command, script, importer).
    Local,
    Undo,
    Redo,
    /// Updates imported from a collaborator (`bae-sync`).
    Remote,
    /// Layer stack or edit target changes.
    Stack,
}

/// Emitted after every committed transaction, undo, redo, remote import or layer
/// stack change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageEvent {
    pub origin: EventOrigin,
    /// `None` for remote imports (authors are in the CRDT history instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<AuthorId>,
    /// Sublayers that were written.
    pub sublayers: Vec<SublayerId>,
    pub changes: Vec<Change>,
}

impl StageEvent {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Whether anything at or below `path` may have changed.
    pub fn touches(&self, path: &PrimPath) -> bool {
        self.changes.iter().any(|c| match c {
            Change::PrimMoved { from, to } => {
                path.is_prefix_of(from)
                    || path.is_prefix_of(to)
                    || from.is_prefix_of(path)
                    || to.is_prefix_of(path)
            }
            Change::PrimRemoved { path: p } | Change::Resynced { path: p } => {
                path.is_prefix_of(p) || p.is_prefix_of(path)
            }
            other => path.is_prefix_of(other.path()),
        })
    }
}
