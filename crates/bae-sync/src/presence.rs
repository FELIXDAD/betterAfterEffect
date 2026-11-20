//! Presence (`docs/spec/sync.md` §6): who is online, what they selected, where
//! their playhead is, which workspace they use, and advisory "I am editing this"
//! locks. Ephemeral: never stored in a document's history.

use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bae_model::{AuthorId, PrimPath, Time};
use serde::{Deserialize, Serialize};

use crate::client::ClientId;
use crate::protocol::Message;

/// What one client shows the others. Unknown fields from newer versions are kept
/// in `extra` and passed on unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresenceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<AuthorId>,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// sRGB display color.
    #[serde(default)]
    pub color: [u8; 3],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selection: Vec<PrimPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playhead: Option<Playhead>,
    /// Workspace in use, e.g. `motion`, `storyboard`, `edit`, `review`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Advisory locks: prims (usually comps) this user says they are working on.
    /// Others get a hint, nobody is blocked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editing: Vec<PrimPath>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Playhead position: which comp, what time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playhead {
    pub comp: PrimPath,
    pub time: Time,
}

/// Another client's presence.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerPresence {
    pub client: ClientId,
    pub state: PresenceState,
}

impl PeerPresence {
    /// Whether this peer declared it is editing `path`, one of its ancestors or
    /// one of its descendants.
    pub fn is_editing(&self, path: &PrimPath) -> bool {
        self.state
            .editing
            .iter()
            .any(|e| e.is_prefix_of(path) || path.is_prefix_of(e))
    }
}

#[derive(Debug)]
struct Entry {
    seq: u64,
    ttl: Duration,
    /// Local time of the last update (liveness never depends on other clocks).
    seen: Instant,
    /// `None`: the client said it left (kept until the TTL so a late, older
    /// heartbeat cannot bring it back).
    state: Option<PresenceState>,
    raw: Message,
}

/// Newest presence per client, expired by the receiver's own clock.
#[derive(Debug, Default)]
pub(crate) struct PresenceTable {
    entries: BTreeMap<ClientId, Entry>,
}

impl PresenceTable {
    /// Apply a `Presence` message. `None`: stale or malformed, ignored.
    /// `Some(changed)`: accepted (forward it); `changed` if what peers see changed
    /// (a heartbeat with the same state only refreshes liveness).
    pub fn apply(&mut self, msg: &Message, now: Instant) -> Option<bool> {
        let Message::Presence {
            client,
            seq,
            ttl_ms,
            state,
        } = msg
        else {
            return None;
        };
        if self.entries.get(client).is_some_and(|e| e.seq >= *seq) {
            return None;
        }
        let parsed = match state {
            Some(bytes) => Some(serde_json::from_slice::<PresenceState>(bytes).ok()?),
            None => None,
        };
        let changed = self.entries.get(client).is_none_or(|e| e.state != parsed);
        self.entries.insert(
            client.clone(),
            Entry {
                seq: *seq,
                ttl: Duration::from_millis(u64::from(*ttl_ms)),
                seen: now,
                state: parsed,
                raw: msg.clone(),
            },
        );
        Some(changed)
    }

    /// A "left" message for `client`, newer than anything it sent, if it is
    /// currently shown as online (the relay sends it when a connection drops).
    pub fn leave(&mut self, client: &ClientId, now: Instant) -> Option<Message> {
        let e = self.entries.get(client)?;
        e.state.as_ref()?;
        let msg = Message::Presence {
            client: client.clone(),
            seq: e.seq + 1,
            ttl_ms: u32::try_from(e.ttl.as_millis()).unwrap_or(u32::MAX),
            state: None,
        };
        self.apply(&msg, now);
        Some(msg)
    }

    /// Drop clients not heard from within their TTL; true if a visible one went.
    pub fn expire(&mut self, now: Instant) -> bool {
        let mut changed = false;
        self.entries.retain(|_, e| {
            let alive = now.saturating_duration_since(e.seen) <= e.ttl;
            changed |= !alive && e.state.is_some();
            alive
        });
        changed
    }

    pub fn peers(&self) -> Vec<PeerPresence> {
        self.entries
            .iter()
            .filter_map(|(client, e)| {
                Some(PeerPresence {
                    client: client.clone(),
                    state: e.state.clone()?,
                })
            })
            .collect()
    }

    /// Latest message per client (including "left"), to forward to newcomers.
    pub fn raw(&self) -> impl Iterator<Item = (&ClientId, &Message)> {
        self.entries.iter().map(|(c, e)| (c, &e.raw))
    }
}

/// First sequence number of a session: wall-clock milliseconds, so a restarted
/// client's updates supersede what it sent before the restart.
pub(crate) fn initial_seq() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1)
}
