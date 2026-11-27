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
///
/// Link errors are returned as they happen: `SyncSession::pump` imports what it
/// received earlier in the same pump before reporting them, so a client that
/// sends its last edits and closes at once does not lose them.
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

    fn close(&mut self) -> Result<(), SyncError> {
        self.inner.close()
    }

    /// The WebSocket's: the relay's session holds document data back while it
    /// is over the connection's `max_backlog`.
    fn backlog(&self) -> usize {
        self.inner.backlog()
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use bae_sync::DocKey;

    use super::*;

    /// The relay's flow control sees what waits in the WebSocket: without it, a
    /// slow client would be sent everything it lacks again on every `Hello`.
    #[test]
    fn the_backlog_is_the_websockets() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            HubTransport::accept(stream, None).unwrap()
        });
        // Never reads.
        let _client = HubTransport::connect(
            &format!("ws://{addr}/projects/x"),
            None,
            Duration::from_secs(5),
        )
        .unwrap();
        let mut link = PeerLink::new(server.join().unwrap(), Access::Write, Duration::MAX);
        assert_eq!(link.backlog(), 0);
        // More than the socket buffers take.
        link.send(Message::Updates {
            doc: DocKey::parse("root").unwrap(),
            version: Default::default(),
            data: vec![1; 64 << 20],
        })
        .unwrap();
        assert!(link.backlog() > 0);
    }
}
