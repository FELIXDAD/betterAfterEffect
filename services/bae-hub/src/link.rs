//! [`PeerLink`]: one client connection as the relay sees it.

use std::time::{Duration, Instant};

use bae_sync::{HubTransport, Message, SyncError, Transport};

use crate::config::Access;

/// Largest message accepted from a read-only connection. Its `Updates` and
/// `Snapshot`s are dropped anyway, so it need not make the hub buffer messages of
/// the full size (`HubLimits::max_message_size`, 256 MiB). 16 MiB still fits
/// the `Hello` of a very large project, and is the largest message builds
/// before `HubLimits` could exchange (one frame each, frames up to 16 MiB).
/// Frames are limited to the same size. A read-only client that sends a larger
/// edit is disconnected (and reconnects).
pub(crate) const READ_ONLY_MAX_MESSAGE: usize = 16 << 20;

/// Wraps the WebSocket ([`HubTransport`], non-blocking after the handshake):
///
/// - read-only connections may say hello, ask for snapshots and publish presence;
///   their `Updates` and `Snapshot`s are dropped, and they may not send messages
///   over [`READ_ONLY_MAX_MESSAGE`];
/// - a connection that sent nothing for `timeout` is closed (clients say `Hello`
///   every few seconds; silence means a dead link that TCP has not noticed).
///   Every byte counts, not only whole messages: a large upload on a slow link
///   takes longer than `timeout`, and the client's hellos wait behind it.
pub(crate) struct PeerLink {
    inner: HubTransport,
    access: Access,
    timeout: Duration,
    last_heard: Instant,
    /// `inner.bytes_received()` when `last_heard` was set.
    heard_bytes: u64,
    dropped_writes: u64,
}

impl PeerLink {
    pub fn new(mut inner: HubTransport, access: Access, timeout: Duration) -> Self {
        if access == Access::Read {
            inner.set_receive_limit(READ_ONLY_MAX_MESSAGE);
        }
        PeerLink {
            heard_bytes: inner.bytes_received(),
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
            let msg = self.inner.recv()?;
            let heard = self.inner.bytes_received();
            if heard != self.heard_bytes {
                self.heard_bytes = heard;
                self.last_heard = Instant::now();
            }
            match msg {
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

    fn backlog(&self) -> usize {
        self.inner.backlog()
    }

    fn max_message_size(&self) -> Option<usize> {
        self.inner.max_message_size()
    }
}
