//! [`PeerLink`]: one client connection as the relay sees it.

use std::time::{Duration, Instant};

use bae_sync::{HubTransport, Message, SyncError, Transport};

use crate::config::Access;

/// Wraps the WebSocket ([`HubTransport`], non-blocking after the handshake):
///
/// - read-only connections may say hello, ask for snapshots and publish presence;
///   their `Updates` and `Snapshot`s are dropped;
/// - a connection that sent nothing for `timeout` is closed (clients say `Hello`
///   every few seconds; silence means a dead link that TCP has not noticed).
pub(crate) struct PeerLink {
    inner: HubTransport,
    access: Access,
    timeout: Duration,
    last_heard: Instant,
    dropped_writes: u64,
}

impl PeerLink {
    pub fn new(inner: HubTransport, access: Access, timeout: Duration) -> Self {
        PeerLink {
            inner,
            access,
            timeout,
            last_heard: Instant::now(),
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
                    self.last_heard = Instant::now();
                    if self.access == Access::Read && is_write(&msg) {
                        if self.dropped_writes == 0 {
                            tracing::info!("read-only client sent edits; dropping them");
                        }
                        self.dropped_writes += 1;
                        continue;
                    }
                    return Ok(Some(msg));
                }
                None if self.last_heard.elapsed() > self.timeout => {
                    return Err(SyncError::Hub(format!(
                        "nothing received for {} s",
                        self.timeout.as_secs()
                    )));
                }
                None => return Ok(None),
            }
        }
    }
}
