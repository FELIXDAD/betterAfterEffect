//! [`PeerLink`]: one client connection as the relay sees it.

use std::time::Duration;

use bae_sync::{HubTransport, Message, SyncError, Transport};

use crate::config::Access;

/// Wraps the WebSocket ([`HubTransport`], non-blocking after the handshake):
///
/// - read-only connections may say hello, ask for snapshots and publish presence;
///   their `Updates` and `Snapshot`s are dropped;
/// - a connection that sent nothing for `timeout` is closed (clients say `Hello`
///   every few seconds; silence means a dead link that TCP has not noticed).
///   Counted in bytes ([`HubTransport::set_idle_timeout`]): a large message
///   that takes longer to upload is not cut off while its bytes arrive.
///
/// Link errors are returned as they happen: `SyncSession::pump` imports what it
/// received earlier in the same pump before reporting them, so a client that
/// sends its last edits and closes at once does not lose them.
pub(crate) struct PeerLink {
    inner: HubTransport,
    access: Access,
    dropped_writes: u64,
}

impl PeerLink {
    pub fn new(mut inner: HubTransport, access: Access, timeout: Duration) -> Self {
        inner.set_idle_timeout(Some(timeout));
        PeerLink {
            inner,
            access,
            dropped_writes: 0,
        }
    }
}

/// Messages that change documents (refused from read-only connections).
fn is_write(msg: &Message) -> bool {
    matches!(msg, Message::Updates { .. } | Message::Snapshot { .. })
}

impl Transport for PeerLink {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        self.inner.send(msg)
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        loop {
            match self.inner.recv()? {
                Some(msg) => {
                    if self.access == Access::Read && is_write(&msg) {
                        if self.dropped_writes == 0 {
                            tracing::info!("read-only client sent edits; dropping them");
                        }
                        self.dropped_writes += 1;
                        continue;
                    }
                    return Ok(Some(msg));
                }
                None => return Ok(None),
            }
        }
    }

    fn close(&mut self) -> Result<(), SyncError> {
        self.inner.close()
    }
}
