//! Hub transport (feature `hub`, `docs/spec/sync.md` §3.1): the sync protocol over
//! WebSocket, one encoded [`Message`] per binary frame.
//!
//! The socket is non-blocking after the handshake, so `send` and `recv` never
//! wait; still, wrap it in a [`crate::ThreadedTransport`] so connecting and large
//! transfers stay off the stage thread.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode, header::AUTHORIZATION};
use tungstenite::{Error as WsError, Message as WsMessage, WebSocket};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

/// Limits of a hub connection, the same in both directions.
///
/// tungstenite's defaults (16 MiB per frame, unlimited write queue) do not fit
/// the protocol: messages are sent as single unfragmented frames, so a sublayer
/// snapshot over 16 MiB was refused by the other side, the link dropped and the
/// same message was sent again after every reconnect; and a peer that never
/// reads made the other side queue without bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubLimits {
    /// Largest encoded protocol message either side sends or accepts (also the
    /// frame limit, since a message is one frame). A whole sublayer snapshot
    /// must fit: a larger one fails to send instead of being refused remotely
    /// (the session still sends its other documents first).
    pub max_message: usize,
    /// Bytes that may wait for a peer that stopped reading. Once more than this
    /// waits and nothing went out for `stall_timeout`, `send` and `recv` fail and
    /// the connection is dropped (a relay drops it at once) instead of growing
    /// without bound. How much waits alone never cuts a peer off: one that
    /// keeps reading, however slowly, gets everything (a session sends all a
    /// newcomer lacks back to back, on a first sync the whole project).
    pub max_backlog: usize,
    /// How long nothing may go out while more than `max_backlog` waits; also
    /// how long [`Transport::close`] waits while nothing moves.
    pub stall_timeout: Duration,
}

impl Default for HubLimits {
    fn default() -> Self {
        HubLimits {
            max_message: 256 << 20,
            max_backlog: 16 << 20,
            stall_timeout: Duration::from_secs(30),
        }
    }
}

/// The socket, counting the bytes written to it: how [`HubTransport`] tells a
/// slow peer (bytes keep leaving) from one that stopped reading.
#[derive(Debug)]
struct Counted {
    stream: TcpStream,
    written: u64,
}

impl Counted {
    fn new(stream: TcpStream) -> Self {
        Counted { stream, written: 0 }
    }
}

impl Read for Counted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for Counted {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.stream.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// One WebSocket connection carrying the sync protocol, client or server side.
pub struct HubTransport {
    ws: WebSocket<Counted>,
    limits: HubLimits,
    /// About how many bytes wait in tungstenite's write buffer.
    unsent: u64,
    /// `Counted::written` when we last looked.
    written: u64,
    /// When bytes last left, or nothing was waiting.
    progress: Instant,
    /// A write failed; reported once what already arrived has been read.
    failed: Option<SyncError>,
}

impl std::fmt::Debug for HubTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubTransport")
            .field("peer", &self.ws.get_ref().stream.peer_addr().ok())
            .finish()
    }
}

fn hub_err(e: impl std::fmt::Display) -> SyncError {
    SyncError::Hub(e.to_string())
}

impl HubTransport {
    /// Connect to a hub at `url` (`ws://host:port/path`; the path and query
    /// select the project, as defined by the hub). `token` is sent as
    /// `Authorization: Bearer <token>`. Blocks for at most about `timeout` per
    /// step (connect, handshake).
    pub fn connect(url: &str, token: Option<&str>, timeout: Duration) -> Result<Self, SyncError> {
        Self::connect_with(url, token, timeout, HubLimits::default())
    }

    /// [`connect`](Self::connect) with explicit size limits.
    pub fn connect_with(
        url: &str,
        token: Option<&str>,
        timeout: Duration,
        limits: HubLimits,
    ) -> Result<Self, SyncError> {
        let mut request = url.into_client_request().map_err(hub_err)?;
        match request.uri().scheme_str() {
            Some("ws") => {}
            Some("wss") => {
                return Err(SyncError::Hub(
                    "wss:// is not supported yet (LAN hub only)".into(),
                ));
            }
            other => return Err(SyncError::Hub(format!("unsupported scheme {other:?}"))),
        }
        if let Some(token) = token {
            let value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(hub_err)?;
            request.headers_mut().insert(AUTHORIZATION, value);
        }
        let host = request
            .uri()
            .host()
            .ok_or_else(|| SyncError::Hub(format!("no host in {url}")))?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let port = request.uri().port_u16().unwrap_or(80);
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(hub_err)?
            .next()
            .ok_or_else(|| SyncError::Hub(format!("cannot resolve {host}")))?;
        let stream = TcpStream::connect_timeout(&addr, timeout).map_err(hub_err)?;
        stream.set_nodelay(true).map_err(hub_err)?;
        stream.set_read_timeout(Some(timeout)).map_err(hub_err)?;
        stream.set_write_timeout(Some(timeout)).map_err(hub_err)?;
        let (ws, _) =
            tungstenite::client::client(request, Counted::new(stream)).map_err(|e| match e {
                tungstenite::HandshakeError::Failure(WsError::Http(resp)) => {
                    SyncError::Hub(format!("hub refused the connection: {}", resp.status()))
                }
                other => hub_err(other),
            })?;
        Self::ready(ws, limits)
    }

    /// Server side of an accepted TCP connection (tests and simple tools; the
    /// production hub is `services/bae-hub`). With `token`, clients must send
    /// `Authorization: Bearer <token>`, others get HTTP 401.
    pub fn accept(stream: TcpStream, token: Option<&str>) -> Result<Self, SyncError> {
        Self::accept_with(stream, token, HubLimits::default())
    }

    /// [`accept`](Self::accept) with explicit size limits.
    pub fn accept_with(
        stream: TcpStream,
        token: Option<&str>,
        limits: HubLimits,
    ) -> Result<Self, SyncError> {
        stream.set_nonblocking(false).map_err(hub_err)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(hub_err)?;
        stream.set_nodelay(true).map_err(hub_err)?;
        let expected = token.map(|t| format!("Bearer {t}"));
        // The callback signature (and its large error type) is tungstenite's.
        #[allow(clippy::result_large_err)]
        let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
            let given = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok());
            match &expected {
                Some(want) if given != Some(want.as_str()) => {
                    let mut err = ErrorResponse::new(Some("bad or missing token".into()));
                    *err.status_mut() = StatusCode::UNAUTHORIZED;
                    Err(err)
                }
                _ => Ok(resp),
            }
        };
        let ws = tungstenite::accept_hdr(Counted::new(stream), check).map_err(hub_err)?;
        Self::ready(ws, limits)
    }

    fn ready(ws: WebSocket<Counted>, limits: HubLimits) -> Result<Self, SyncError> {
        let s = &ws.get_ref().stream;
        s.set_read_timeout(None).map_err(hub_err)?;
        s.set_write_timeout(None).map_err(hub_err)?;
        s.set_nonblocking(true).map_err(hub_err)?;
        let written = ws.get_ref().written;
        let mut transport = HubTransport {
            ws,
            limits,
            unsent: 0,
            written,
            progress: Instant::now(),
            failed: None,
        };
        transport.set_limits(limits);
        Ok(transport)
    }

    /// Change the limits of an open connection.
    pub fn set_limits(&mut self, limits: HubLimits) {
        self.limits = limits;
        let max_message = limits.max_message.max(1);
        self.ws.set_config(|c| {
            c.max_message_size = Some(max_message);
            c.max_frame_size = Some(max_message);
            // A peer that stopped reading is told by the lack of progress
            // (`track`), not by how much waits.
            c.max_write_buffer_size = usize::MAX;
        });
    }

    /// The connection's limits.
    pub fn limits(&self) -> HubLimits {
        self.limits
    }

    /// Account for what left since the last call (`drained`: nothing waits any
    /// more). Fails when more than `max_backlog` bytes wait and nothing left for
    /// `stall_timeout`.
    fn track(&mut self, drained: bool) -> Result<(), SyncError> {
        let now = Instant::now();
        let written = self.ws.get_ref().written;
        let moved = written - self.written;
        self.written = written;
        self.unsent = if drained {
            0
        } else {
            self.unsent.saturating_sub(moved)
        };
        if moved > 0 || self.unsent == 0 {
            self.progress = now;
        } else if self.unsent > self.limits.max_backlog as u64
            && now.saturating_duration_since(self.progress) >= self.limits.stall_timeout
        {
            return Err(SyncError::Hub(format!(
                "peer is not reading: {} KiB waiting, nothing sent for {} s",
                self.unsent >> 10,
                self.limits.stall_timeout.as_secs_f32()
            )));
        }
        Ok(())
    }
}

fn would_block(e: &WsError) -> bool {
    matches!(e, WsError::Io(io) if io.kind() == std::io::ErrorKind::WouldBlock)
}

fn link_err(e: WsError) -> SyncError {
    match e {
        WsError::ConnectionClosed | WsError::AlreadyClosed => SyncError::Closed,
        WsError::Io(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            SyncError::Closed
        }
        other => hub_err(other),
    }
}

impl Transport for HubTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let bytes = msg.encode();
        // tungstenite only checks the size of incoming messages; the peer would
        // drop the link over an oversized one, so fail here with a clear reason.
        if let Some(max) = self.ws.get_config().max_message_size
            && bytes.len() > max
        {
            return Err(SyncError::Hub(format!(
                "message of {} bytes is over the {max}-byte limit of the hub link",
                bytes.len()
            )));
        }
        // Frame header included (at most 14 bytes).
        self.unsent += bytes.len() as u64 + 14;
        let drained = match self.ws.send(WsMessage::Binary(bytes.into())) {
            Ok(()) => true,
            // Queued; the rest goes out on later flushes.
            Err(e) if would_block(&e) => false,
            Err(e) => return Err(link_err(e)),
        };
        self.track(drained)
    }

    /// A write that failed (the peer reset the connection, or stopped reading)
    /// is reported only once nothing that already arrived is left to read: the
    /// peer counts those messages as delivered (a client that sends its last
    /// edits and closes at once while we are still sending to it). Best effort:
    /// some systems (Windows) drop unread data when a reset arrives, which is
    /// why peers should end with [`Transport::close`].
    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        if self.failed.is_none() {
            let flushed = match self.ws.flush() {
                Ok(()) => self.track(true),
                Err(e) if would_block(&e) => self.track(false),
                Err(e) => Err(link_err(e)),
            };
            self.failed = flushed.err();
        }
        loop {
            match self.ws.read() {
                Ok(WsMessage::Binary(bytes)) => match Message::decode(&bytes) {
                    Ok(m) => return Ok(Some(m)),
                    // Newer message types and garbage are skipped.
                    Err(_) => continue,
                },
                Ok(WsMessage::Close(_)) => return Err(SyncError::Closed),
                // Pings are answered by tungstenite on the next flush.
                Ok(_) => continue,
                Err(e) if would_block(&e) => {
                    return match self.failed.take() {
                        Some(e) => Err(e),
                        None => Ok(None),
                    };
                }
                Err(e) => return Err(self.failed.take().unwrap_or_else(|| link_err(e))),
            }
        }
    }

    /// Sends what waits and a close frame, then reads (and drops) what arrives
    /// until the peer answers the close: by then it has read everything we
    /// sent, and our socket has nothing unread left, so closing it does not
    /// reset the connection (a reset can throw away what the peer has not read
    /// yet). Gives up with an error once nothing went out for
    /// [`HubLimits::stall_timeout`].
    fn close(&mut self) -> Result<(), SyncError> {
        if let Some(e) = self.failed.take() {
            return Err(e);
        }
        let mut written = self.ws.get_ref().written;
        let mut progress = Instant::now();
        let mut r = self.ws.close(None);
        loop {
            match r {
                // tungstenite saw the close handshake through.
                Err(WsError::ConnectionClosed) => return Ok(()),
                Err(e) if !would_block(&e) => return Err(link_err(e)),
                _ => {}
            }
            r = loop {
                match self.ws.read() {
                    // The peer's answer (we sent our close frame first).
                    Ok(WsMessage::Close(_)) => return Ok(()),
                    Ok(_) => continue,
                    Err(e) => break Err(e),
                }
            };
            if matches!(&r, Err(e) if would_block(e)) {
                let now_written = self.ws.get_ref().written;
                if now_written != written {
                    (written, progress) = (now_written, Instant::now());
                } else if progress.elapsed() >= self.limits.stall_timeout {
                    return Err(SyncError::Hub(
                        "the peer did not confirm closing the connection".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
                r = self.ws.flush();
            }
        }
    }
}

impl Drop for HubTransport {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}
