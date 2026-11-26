//! Sync protocol messages and their binary encoding (`docs/spec/sync.md`).
//!
//! The protocol is the same for every transport: shared-folder logs store encoded
//! [`Message::Updates`] / [`Message::Snapshot`] records, and the hub carries every
//! message as one WebSocket binary message.

use std::fmt;

use bae_model::SublayerId;
use bae_model::loro::VersionVector;

use crate::client::ClientId;
use crate::error::SyncError;

/// Version of the sync protocol spoken by this build, sent in [`Message::Hello`].
pub const PROTOCOL_VERSION: u32 = 1;

/// A document synchronized by the protocol: one sublayer, or the project history
/// (named versions).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DocKey {
    Sublayer(SublayerId),
    /// Named versions and other project-wide history metadata.
    History,
}

impl DocKey {
    /// Wire name of [`DocKey::History`]. `@` cannot appear in sublayer IDs.
    pub const HISTORY: &'static str = "@history";

    pub fn sublayer(id: &SublayerId) -> Self {
        DocKey::Sublayer(id.clone())
    }

    /// Wire and file-system name: the sublayer ID, or `@history`.
    pub fn as_str(&self) -> &str {
        match self {
            DocKey::Sublayer(id) => id.as_str(),
            DocKey::History => Self::HISTORY,
        }
    }

    pub fn parse(s: &str) -> Result<Self, SyncError> {
        if s == Self::HISTORY {
            return Ok(DocKey::History);
        }
        SublayerId::new(s)
            .map(DocKey::Sublayer)
            .map_err(|_| SyncError::InvalidDocKey(s.to_owned()))
    }

    pub fn as_sublayer(&self) -> Option<&SublayerId> {
        match self {
            DocKey::Sublayer(id) => Some(id),
            DocKey::History => None,
        }
    }
}

impl fmt::Display for DocKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for DocKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DocKey({})", self.as_str())
    }
}

/// One protocol message.
///
/// Version vectors say "the sender has every op up to here": a receiver that already
/// has everything in `version` can skip the payload, and after receiving it knows
/// the sender has at least that much.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub enum Message {
    /// "This is what I have." Sent on connect and periodically; the receiver answers
    /// with [`Message::Updates`] for whatever the sender is missing. Documents not
    /// listed count as empty.
    Hello {
        protocol: u32,
        client: ClientId,
        versions: Vec<(DocKey, VersionVector)>,
    },
    /// Loro updates for one document. `version`: the sender's version after them.
    Updates {
        doc: DocKey,
        version: VersionVector,
        data: Vec<u8>,
    },
    /// Ask for a full snapshot of `doc` (shared-folder mode uses it to compact the
    /// client's own log).
    SnapshotRequest { doc: DocKey },
    /// A full Loro snapshot (history included) of `doc` at `version`.
    Snapshot {
        doc: DocKey,
        version: VersionVector,
        data: Vec<u8>,
    },
    /// Ephemeral presence of `client` (who is online, what they look at); not part
    /// of any document's history. `seq` grows with every update, receivers keep
    /// the newest. The client counts as gone after `ttl_ms` without an update.
    /// `state`: JSON of [`crate::PresenceState`]; `None` when the client left.
    Presence {
        client: ClientId,
        seq: u64,
        ttl_ms: u32,
        state: Option<Vec<u8>>,
    },
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::Hello {
                protocol,
                client,
                versions,
            } => f
                .debug_struct("Hello")
                .field("protocol", protocol)
                .field("client", client)
                .field("docs", &versions.len())
                .finish(),
            Message::Updates { doc, data, .. } => f
                .debug_struct("Updates")
                .field("doc", doc)
                .field("bytes", &data.len())
                .finish_non_exhaustive(),
            Message::SnapshotRequest { doc } => {
                f.debug_struct("SnapshotRequest").field("doc", doc).finish()
            }
            Message::Snapshot { doc, data, .. } => f
                .debug_struct("Snapshot")
                .field("doc", doc)
                .field("bytes", &data.len())
                .finish_non_exhaustive(),
            Message::Presence {
                client, seq, state, ..
            } => f
                .debug_struct("Presence")
                .field("client", client)
                .field("seq", seq)
                .field("left", &state.is_none())
                .finish_non_exhaustive(),
        }
    }
}

const TAG_HELLO: u8 = 1;
const TAG_UPDATES: u8 = 2;
const TAG_SNAPSHOT_REQUEST: u8 = 3;
const TAG_SNAPSHOT: u8 = 4;
const TAG_PRESENCE: u8 = 5;

impl Message {
    pub fn hello(client: ClientId, versions: Vec<(DocKey, VersionVector)>) -> Self {
        Message::Hello {
            protocol: PROTOCOL_VERSION,
            client,
            versions,
        }
    }

    /// The document this message is about (`None` for `Hello`).
    pub fn doc(&self) -> Option<&DocKey> {
        match self {
            Message::Hello { .. } | Message::Presence { .. } => None,
            Message::Updates { doc, .. }
            | Message::SnapshotRequest { doc }
            | Message::Snapshot { doc, .. } => Some(doc),
        }
    }

    /// Binary encoding: a tag byte, then the fields; strings and byte arrays are
    /// LEB128-length-prefixed, version vectors use Loro's encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(match self {
            Message::Updates { data, .. } | Message::Snapshot { data, .. } => data.len() + 64,
            _ => 64,
        });
        match self {
            Message::Hello {
                protocol,
                client,
                versions,
            } => {
                w.push(TAG_HELLO);
                put_varint(&mut w, u64::from(*protocol));
                put_bytes(&mut w, client.as_str().as_bytes());
                put_varint(&mut w, versions.len() as u64);
                for (doc, vv) in versions {
                    put_bytes(&mut w, doc.as_str().as_bytes());
                    put_bytes(&mut w, &vv.encode());
                }
            }
            Message::Updates { doc, version, data } | Message::Snapshot { doc, version, data } => {
                let tag = if matches!(self, Message::Updates { .. }) {
                    TAG_UPDATES
                } else {
                    TAG_SNAPSHOT
                };
                put_data_head(&mut w, tag, doc, version, data.len());
                w.extend_from_slice(data);
            }
            Message::SnapshotRequest { doc } => {
                w.push(TAG_SNAPSHOT_REQUEST);
                put_bytes(&mut w, doc.as_str().as_bytes());
            }
            Message::Presence {
                client,
                seq,
                ttl_ms,
                state,
            } => {
                w.push(TAG_PRESENCE);
                put_bytes(&mut w, client.as_str().as_bytes());
                put_varint(&mut w, *seq);
                put_varint(&mut w, u64::from(*ttl_ms));
                match state {
                    Some(s) => {
                        w.push(1);
                        put_bytes(&mut w, s);
                    }
                    None => w.push(0),
                }
            }
        }
        w
    }

    /// [`encode`](Self::encode) in two parts, `head` then `data`, without copying
    /// the payload of `Updates` and `Snapshot` (`data` is that payload, empty for
    /// other messages). For transports that send large messages piece by piece.
    pub(crate) fn into_encoded_parts(self) -> (Vec<u8>, Vec<u8>) {
        let tag = match &self {
            Message::Updates { .. } => TAG_UPDATES,
            Message::Snapshot { .. } => TAG_SNAPSHOT,
            _ => return (self.encode(), Vec::new()),
        };
        match self {
            Message::Updates { doc, version, data } | Message::Snapshot { doc, version, data } => {
                let mut head = Vec::with_capacity(64);
                put_data_head(&mut head, tag, &doc, &version, data.len());
                (head, data)
            }
            _ => unreachable!("data messages only"),
        }
    }

    /// How many bytes [`encode`](Self::encode) produces, without copying the
    /// payload of `Updates` and `Snapshot`.
    pub fn encoded_len(&self) -> usize {
        match self {
            Message::Updates { doc, version, data } | Message::Snapshot { doc, version, data } => {
                let mut head = Vec::with_capacity(64);
                put_data_head(&mut head, TAG_UPDATES, doc, version, data.len());
                head.len() + data.len()
            }
            _ => self.encode().len(),
        }
    }

    /// Decode one message. Bytes after the known fields are ignored, so later
    /// protocol versions can append fields. An unknown tag is
    /// [`SyncError::UnknownMessage`], which receivers should skip.
    pub fn decode(bytes: &[u8]) -> Result<Self, SyncError> {
        let mut r = Reader { bytes, pos: 0 };
        let tag = r.u8()?;
        Ok(match tag {
            TAG_HELLO => {
                let protocol = u32::try_from(r.varint()?)
                    .map_err(|_| SyncError::Decode("protocol version out of range".into()))?;
                let client = ClientId::new(r.str()?)?;
                let n = r.varint()?;
                let mut versions = Vec::with_capacity(n.min(1024) as usize);
                for _ in 0..n {
                    let doc = DocKey::parse(r.str()?)?;
                    versions.push((doc, r.vv()?));
                }
                Message::Hello {
                    protocol,
                    client,
                    versions,
                }
            }
            TAG_UPDATES | TAG_SNAPSHOT => {
                let doc = DocKey::parse(r.str()?)?;
                let version = r.vv()?;
                let data = r.bytes()?.to_vec();
                if tag == TAG_UPDATES {
                    Message::Updates { doc, version, data }
                } else {
                    Message::Snapshot { doc, version, data }
                }
            }
            TAG_SNAPSHOT_REQUEST => Message::SnapshotRequest {
                doc: DocKey::parse(r.str()?)?,
            },
            TAG_PRESENCE => {
                let client = ClientId::new(r.str()?)?;
                let seq = r.varint()?;
                let ttl_ms = u32::try_from(r.varint()?)
                    .map_err(|_| SyncError::Decode("presence ttl out of range".into()))?;
                let state = match r.u8()? {
                    0 => None,
                    1 => Some(r.bytes()?.to_vec()),
                    _ => return Err(SyncError::Decode("bad presence flag".into())),
                };
                Message::Presence {
                    client,
                    seq,
                    ttl_ms,
                    state,
                }
            }
            other => return Err(SyncError::UnknownMessage(other)),
        })
    }
}

pub(crate) fn put_varint(w: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            w.push(byte);
            return;
        }
        w.push(byte | 0x80);
    }
}

/// Everything of an `Updates` / `Snapshot` encoding before the payload bytes,
/// including the payload's length.
fn put_data_head(w: &mut Vec<u8>, tag: u8, doc: &DocKey, version: &VersionVector, len: usize) {
    w.push(tag);
    put_bytes(w, doc.as_str().as_bytes());
    put_bytes(w, &version.encode());
    put_varint(w, len as u64);
}

fn put_bytes(w: &mut Vec<u8>, b: &[u8]) {
    put_varint(w, b.len() as u64);
    w.extend_from_slice(b);
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, SyncError> {
        let b = *self
            .bytes
            .get(self.pos)
            .ok_or_else(|| SyncError::Decode("unexpected end of message".into()))?;
        self.pos += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u64, SyncError> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(SyncError::Decode("varint too long".into()))
    }

    fn bytes(&mut self) -> Result<&'a [u8], SyncError> {
        let len = self.varint()?;
        let end = usize::try_from(len)
            .ok()
            .and_then(|l| self.pos.checked_add(l))
            .filter(|&e| e <= self.bytes.len())
            .ok_or_else(|| SyncError::Decode("length exceeds message".into()))?;
        let b = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(b)
    }

    fn str(&mut self) -> Result<&'a str, SyncError> {
        std::str::from_utf8(self.bytes()?).map_err(|_| SyncError::Decode("invalid UTF-8".into()))
    }

    fn vv(&mut self) -> Result<VersionVector, SyncError> {
        VersionVector::decode(self.bytes()?)
            .map_err(|e| SyncError::Decode(format!("version vector: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::loro::LoroDoc;

    fn vv_with_ops() -> VersionVector {
        let doc = LoroDoc::new();
        doc.get_map("m").insert("k", 1).unwrap();
        doc.commit();
        doc.oplog_vv()
    }

    #[test]
    fn doc_keys() {
        assert_eq!(
            DocKey::parse("anim/shot010").unwrap().as_str(),
            "anim/shot010"
        );
        assert_eq!(DocKey::parse("@history").unwrap(), DocKey::History);
        assert!(DocKey::parse("@other").is_err());
        assert!(DocKey::parse("../x").is_err());
    }

    #[test]
    fn round_trip() {
        let vv = vv_with_ops();
        let msgs = [
            Message::hello(
                ClientId::new("alice").unwrap(),
                vec![
                    (DocKey::parse("root").unwrap(), vv.clone()),
                    (DocKey::History, VersionVector::new()),
                ],
            ),
            Message::Updates {
                doc: DocKey::parse("anim/shot010").unwrap(),
                version: vv.clone(),
                data: vec![1, 2, 3],
            },
            Message::SnapshotRequest {
                doc: DocKey::History,
            },
            Message::Snapshot {
                doc: DocKey::parse("root").unwrap(),
                version: vv,
                data: vec![0; 300],
            },
            Message::Presence {
                client: ClientId::new("bob").unwrap(),
                seq: 300,
                ttl_ms: 20_000,
                state: Some(br#"{"name":"Bob"}"#.to_vec()),
            },
            Message::Presence {
                client: ClientId::new("bob").unwrap(),
                seq: 301,
                ttl_ms: 20_000,
                state: None,
            },
        ];
        for m in msgs {
            let bytes = m.encode();
            assert_eq!(Message::decode(&bytes).unwrap(), m);
            // The two-part encoding is the same bytes.
            let (head, data) = m.clone().into_encoded_parts();
            assert_eq!([head, data].concat(), bytes);
            assert_eq!(m.encoded_len(), bytes.len(), "{m:?}");
            // Appended fields from a newer protocol version are ignored.
            let mut longer = bytes.clone();
            longer.extend_from_slice(&[9, 9, 9]);
            assert_eq!(Message::decode(&longer).unwrap(), m);
            // Every truncation is an error, never a panic.
            for cut in 0..bytes.len() {
                assert!(Message::decode(&bytes[..cut]).is_err(), "cut at {cut}");
            }
        }
        assert!(matches!(
            Message::decode(&[200, 1, 2]),
            Err(SyncError::UnknownMessage(200))
        ));
    }
}
