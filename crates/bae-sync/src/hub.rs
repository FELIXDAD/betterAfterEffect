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

/// Limits of one hub connection (`docs/spec/sync.md` §3.1).
///
/// tungstenite's defaults (16 MiB per frame, unlimited write queue) do not fit
/// the protocol: messages are sent as single unfragmented frames, so a sublayer
/// snapshot over 16 MiB was refused by the other side, the link dropped and the
/// same message was sent again after every reconnect; and a peer that never
/// reads made the other side queue without bound.
///
/// What waits for the peer is bounded twice. Sessions send no document data
/// while more than `max_backlog` waits ([`Transport::backlog`],
/// [`crate::SessionConfig::max_backlog`]), so a peer that reads slowly holds
/// at most `max_backlog` plus one message (and the `Hello`s, presence and
/// snapshot answers sent meanwhile, which are small or rare). Whatever sends,
/// more than `max_unsent` never waits: the link fails instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubLimits {
    /// Largest encoded protocol message this side sends. A whole sublayer
    /// snapshot must fit: a larger one fails to send instead of being refused
    /// remotely (the session still sends its other documents first). The
    /// peer's `max_incoming` must not be smaller (except for document data the
    /// peer will not take anyway: a read-only hub connection, whose session
    /// sends none, [`crate::SessionConfig::send_data`]).
    pub max_message: usize,
    /// Largest message accepted from the peer (also the frame limit, since a
    /// message is one frame): a larger one fails `recv` as soon as its frame
    /// header arrives, before it is buffered, and the link is dropped. The hub
    /// takes less from read-only connections (16 MiB by default), whose
    /// sessions should send no document data.
    pub max_incoming: usize,
    /// Flow control: what may wait for a peer that reads slowly before sessions
    /// hold back further document data (see above; sessions have their own
    /// setting, [`crate::SessionConfig::max_backlog`], with the same default).
    /// Also the stall rule: once more than this waits and nothing went out for
    /// `stall_timeout`, `send` and `recv` fail and the connection is dropped
    /// (a relay drops it at once). How much waits alone never cuts a peer off
    /// below `max_unsent`: one that keeps reading, however slowly, gets
    /// everything, one held-back batch after the other.
    pub max_backlog: usize,
    /// Hard cap on what waits for the peer: a `send` that would make more than
    /// this wait fails, and so does the link from then on (a relay drops it).
    /// A message that finds nothing waiting is always taken, so any message up
    /// to `max_message` can be sent. Sessions that hold back at `max_backlog`
    /// stay well below it; it stops what does not (snapshot requests sent over
    /// and over, a session without flow control).
    pub max_unsent: usize,
    /// How long nothing may go out while more than `max_backlog` waits; also
    /// how long [`Transport::close`] waits while nothing moves.
    pub stall_timeout: Duration,
}

impl Default for HubLimits {
    /// 256 MiB messages both ways, flow control at 16 MiB, at most 528 MiB
    /// waiting (room for what a held-back session leaves waiting, one message
    /// over `max_backlog`, plus one snapshot answer), 30 s stall timeout.
    fn default() -> Self {
        const MAX_MESSAGE: usize = 256 << 20;
        HubLimits {
            max_message: MAX_MESSAGE,
            max_incoming: MAX_MESSAGE,
            max_backlog: crate::session::DEFAULT_MAX_BACKLOG,
            max_unsent: 2 * MAX_MESSAGE + crate::session::DEFAULT_MAX_BACKLOG,
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
    /// More than `max_unsent` would have waited: every later call fails with
    /// this (`recv` once what already arrived has been read).
    over: Option<String>,
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
            over: None,
        };
        transport.set_limits(limits);
        Ok(transport)
    }

    /// Change the limits of an open connection.
    pub fn set_limits(&mut self, limits: HubLimits) {
        self.limits = limits;
        let max_incoming = limits.max_incoming.max(1);
        self.ws.set_config(|c| {
            c.max_message_size = Some(max_incoming);
            c.max_frame_size = Some(max_incoming);
            // What waits is bounded by `send` (`max_unsent`) and a peer that
            // stopped reading is told by the lack of progress (`track`).
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
        if let Some(over) = &self.over {
            return Err(SyncError::Hub(over.clone()));
        }
        let bytes = msg.encode();
        // tungstenite only checks the size of incoming messages; the peer would
        // drop the link over an oversized one, so fail here with a clear reason.
        let max = self.limits.max_message;
        if bytes.len() > max {
            return Err(SyncError::Hub(format!(
                "message of {} bytes is over the {max}-byte limit of the hub link",
                bytes.len()
            )));
        }
        // Frame header included (at most 14 bytes).
        let len = bytes.len() as u64 + 14;
        if self.unsent > 0 && self.unsent + len > self.limits.max_unsent as u64 {
            let over = format!(
                "{} KiB would wait for the peer, over the limit of {} KiB",
                (self.unsent + len) >> 10,
                self.limits.max_unsent >> 10
            );
            self.over = Some(over.clone());
            return Err(SyncError::Hub(over));
        }
        self.unsent += len;
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
                    return match (self.failed.take(), &self.over) {
                        (Some(e), _) => Err(e),
                        (None, Some(over)) => Err(SyncError::Hub(over.clone())),
                        (None, None) => Ok(None),
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
        if let Some(over) = &self.over {
            return Err(SyncError::Hub(over.clone()));
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

    /// About how many bytes wait in the write buffer (not counting the
    /// system's socket buffers), as of the last `send` or `recv`.
    fn backlog(&self) -> usize {
        usize::try_from(self.unsent).unwrap_or(usize::MAX)
    }
}

impl Drop for HubTransport {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}
