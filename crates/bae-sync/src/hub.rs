//! Hub transport (feature `hub`, `docs/spec/sync.md` §3.1): the sync protocol over
//! WebSocket, one encoded [`Message`] per binary WebSocket message.
//!
//! The socket is non-blocking after the handshake, so `send` and `recv` never
//! wait. Outgoing messages wait in the transport's own queue and go out in frames
//! of at most 256 KiB whenever the socket takes more (on `send` and on every
//! `recv`); [`Transport::backlog`] says how much is waiting. Still, wrap
//! it in a [`crate::ThreadedTransport`] so connecting and large transfers stay
//! off the stage thread.
//!
//! Message size and the send queue are bounded by [`HubLimits`]; both ends of a
//! link must use the same message size limit.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::error::{CapacityError, ProtocolError};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode, header::AUTHORIZATION};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::protocol::frame::Frame;
use tungstenite::protocol::frame::coding::{Data, OpCode};
use tungstenite::{Bytes, Error as WsError, Message as WsMessage, WebSocket};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

/// Largest WebSocket frame this side sends: a message is split into frames of
/// this size. tungstenite copies each frame into its own write buffer and never
/// shrinks that buffer again, so whole large messages would keep their size in
/// memory for the life of the connection (and be moved byte by byte on every
/// partial write).
const FRAME_SIZE: usize = 256 << 10;
/// What the smallest write buffer cap allows on top of one message of the
/// largest size: the session starts a message with up to
/// [`crate::SessionConfig::DEFAULT_MAX_BACKLOG`] (1 MiB) still queued, and small
/// messages queue up behind it.
const MIN_WRITE_BUFFER_MARGIN: usize = 2 << 20;

/// Size limits of a hub connection (`docs/spec/sync.md` §3.1).
///
/// Both ends of a link must use the same `max_message_size`. A message larger
/// than the receiver accepts closes the link, and after reconnecting the same
/// message is sent again, forever. [`HubTransport::connect`],
/// [`HubTransport::accept`] and `services/bae-hub` (key `max_message_mib` in
/// `hub.toml`) all default to [`HubLimits::default`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubLimits {
    /// Largest message sent or accepted, in bytes (encoded [`Message`]). Sending
    /// a larger one fails with [`SyncError::MessageTooLarge`] (only that message;
    /// the session skips its document); receiving one closes the link. Default
    /// [`DEFAULT_MAX_MESSAGE_SIZE`](Self::DEFAULT_MAX_MESSAGE_SIZE).
    pub max_message_size: usize,
    /// Most bytes waiting to be sent ([`Transport::backlog`]: queued in the
    /// transport, not yet taken by the socket). A message that does not fit
    /// makes `send` fail and the link must be re-established, so the caller
    /// (e.g. [`crate::Relay`]) drops a peer that stopped reading.
    ///
    /// The session normally keeps the backlog far below this: it sends no local
    /// changes while more than [`crate::SessionConfig::max_backlog`] (1 MiB) is
    /// waiting, so at most one large message and a few small ones are queued.
    /// Raised to [`min_write_buffer_size`](Self::min_write_buffer_size) when
    /// smaller; see [`write_buffer_cap`](Self::write_buffer_cap). Default
    /// `max_message_size` + [`WRITE_BUFFER_MARGIN`](Self::WRITE_BUFFER_MARGIN).
    pub max_write_buffer_size: usize,
}

impl HubLimits {
    /// Default [`max_message_size`](Self::max_message_size): 256 MiB, enough for
    /// the snapshot of a very large sublayer.
    pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 256 << 20;
    /// What the default write buffer cap allows on top of one message of the
    /// largest size: room for the small messages (hellos, presence) that queue up
    /// while a large one drains to a slow peer.
    pub const WRITE_BUFFER_MARGIN: usize = 16 << 20;

    /// `max_message_size` with the default write buffer cap for it.
    pub fn with_max_message_size(max_message_size: usize) -> Self {
        HubLimits {
            max_message_size,
            max_write_buffer_size: max_message_size.saturating_add(Self::WRITE_BUFFER_MARGIN),
        }
    }

    /// Smallest write buffer cap that still lets a message of the largest size
    /// through when a session starts it: the message, plus 2 MiB for what may
    /// still be queued then (the session's default backlog limit of 1 MiB) and
    /// for small messages behind it.
    pub fn min_write_buffer_size(&self) -> usize {
        self.max_message_size
            .saturating_add(MIN_WRITE_BUFFER_MARGIN)
    }

    /// The write buffer cap in use: `max_write_buffer_size`, but at least
    /// [`min_write_buffer_size`](Self::min_write_buffer_size).
    pub fn write_buffer_cap(&self) -> usize {
        self.max_write_buffer_size.max(self.min_write_buffer_size())
    }

    /// tungstenite's settings: messages, and frames, up to `max_message_size`
    /// on receipt. Ours are [`FRAME_SIZE`], but builds before `HubLimits` send
    /// each message as one frame (and accept messages up to 64 MiB): a message
    /// one of them sends or relays arrives as a single frame of its full size.
    /// tungstenite reserves a frame's length when its header arrives, after
    /// checking it against this limit, so the reservation is bounded like the
    /// message. Its own write buffer stays unbounded but never holds more than
    /// one of our frames ([`HubTransport`] hands it the next only once it wrote
    /// the last).
    fn ws_config(&self) -> WebSocketConfig {
        WebSocketConfig::default()
            .max_message_size(Some(self.max_message_size))
            .max_frame_size(Some(self.max_message_size))
    }
}

impl Default for HubLimits {
    fn default() -> Self {
        Self::with_max_message_size(Self::DEFAULT_MAX_MESSAGE_SIZE)
    }
}

/// The TCP stream, counting what goes through it: bytes received tell a slow
/// link from a dead one while a large message arrives, bytes sent show progress.
#[derive(Debug)]
struct Counted {
    tcp: TcpStream,
    read: u64,
    written: u64,
}

impl Counted {
    fn new(tcp: TcpStream) -> Self {
        Counted {
            tcp,
            read: 0,
            written: 0,
        }
    }
}

impl Read for Counted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.tcp.read(buf)?;
        self.read += n as u64;
        Ok(n)
    }
}

impl Write for Counted {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.tcp.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

/// One encoded message waiting to be sent: `parts` one after the other (the
/// payload of a large message stays in its own buffer, not copied).
struct Outgoing {
    parts: [Bytes; 2],
    /// Bytes already handed to tungstenite.
    pos: usize,
}

impl Outgoing {
    fn len(&self) -> usize {
        self.parts[0].len() + self.parts[1].len()
    }

    /// The next frame's payload: up to `max` bytes from `pos`. Copies only a
    /// frame that spans both parts.
    fn next_frame(&mut self, max: usize) -> Bytes {
        let head = self.parts[0].len();
        let (start, end) = (self.pos, (self.pos + max).min(self.len()));
        self.pos = end;
        if end <= head {
            self.parts[0].slice(start..end)
        } else if start >= head {
            self.parts[1].slice(start - head..end - head)
        } else {
            let mut joined = Vec::with_capacity(end - start);
            joined.extend_from_slice(&self.parts[0][start..]);
            joined.extend_from_slice(&self.parts[1][..end - head]);
            joined.into()
        }
    }
}

/// One WebSocket connection carrying the sync protocol, client or server side.
pub struct HubTransport {
    ws: WebSocket<Counted>,
    limits: HubLimits,
    /// Messages waiting to be handed to tungstenite, oldest first.
    queue: VecDeque<Outgoing>,
    /// Bytes in `queue` not yet handed to tungstenite.
    queued: usize,
    /// Size of the last frame handed to tungstenite while its write has not
    /// completed (it holds at most this one frame of ours).
    unflushed: usize,
}

impl std::fmt::Debug for HubTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubTransport")
            .field("peer", &self.ws.get_ref().tcp.peer_addr().ok())
            .field("backlog", &self.backlog())
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
    /// step (connect, handshake). Uses [`HubLimits::default`], like the hub.
    pub fn connect(url: &str, token: Option<&str>, timeout: Duration) -> Result<Self, SyncError> {
        Self::connect_with(url, token, timeout, HubLimits::default())
    }

    /// [`connect`](Self::connect) with other size limits (the hub must use the
    /// same message size limit).
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
        let (ws, _) = tungstenite::client::client_with_config(
            request,
            Counted::new(stream),
            Some(limits.ws_config()),
        )
        .map_err(|e| match e {
            tungstenite::HandshakeError::Failure(WsError::Http(resp)) => {
                SyncError::Hub(format!("hub refused the connection: {}", resp.status()))
            }
            other => hub_err(other),
        })?;
        Self::ready(ws, limits)
    }

    /// Server side of an accepted TCP connection (tests and simple tools; the
    /// production hub is `services/bae-hub`). With `token`, clients must send
    /// `Authorization: Bearer <token>`, others get HTTP 401. Uses
    /// [`HubLimits::default`], like the clients.
    pub fn accept(stream: TcpStream, token: Option<&str>) -> Result<Self, SyncError> {
        Self::accept_with(stream, token, HubLimits::default())
    }

    /// [`accept`](Self::accept) with other size limits (the clients must use the
    /// same message size limit).
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
        let ws = tungstenite::accept_hdr_with_config(
            Counted::new(stream),
            check,
            Some(limits.ws_config()),
        )
        .map_err(hub_err)?;
        Self::ready(ws, limits)
    }

    fn ready(ws: WebSocket<Counted>, limits: HubLimits) -> Result<Self, SyncError> {
        let s = &ws.get_ref().tcp;
        s.set_read_timeout(None).map_err(hub_err)?;
        s.set_write_timeout(None).map_err(hub_err)?;
        s.set_nonblocking(true).map_err(hub_err)?;
        Ok(HubTransport {
            ws,
            limits,
            queue: VecDeque::new(),
            queued: 0,
            unflushed: 0,
        })
    }

    /// The size limits of this connection.
    pub fn limits(&self) -> HubLimits {
        self.limits
    }

    /// From now on accept messages (and frames) of at most `max` bytes (at most
    /// [`HubLimits::max_message_size`]); a larger one closes the link as soon
    /// as its size shows: at the header of a frame that large, or once that
    /// much of a message has arrived. Messages sent keep the limit of
    /// [`limits`](Self::limits). The hub uses it for read-only connections,
    /// whose edits it drops anyway, so they cannot make it buffer large ones.
    pub fn set_receive_limit(&mut self, max: usize) {
        let max = max.min(self.limits.max_message_size);
        self.ws.set_config(|c| {
            c.max_message_size = Some(max);
            c.max_frame_size = Some(max);
        });
    }

    /// Bytes received on this connection so far (the handshake included). Grows
    /// while a large message arrives, long before `recv` returns it: liveness
    /// checks use it to tell a slow link from a dead one.
    pub fn bytes_received(&self) -> u64 {
        self.ws.get_ref().read
    }

    /// Bytes the socket has taken so far (the handshake included).
    pub fn bytes_sent(&self) -> u64 {
        self.ws.get_ref().written
    }

    /// Hand queued frames to tungstenite for as long as the socket takes them.
    /// tungstenite gets the next frame only once it wrote the previous one, so
    /// its write buffer never holds more than one frame.
    fn write_out(&mut self) -> Result<(), WsError> {
        loop {
            match self.ws.flush() {
                Ok(()) => self.unflushed = 0,
                Err(e) if would_block(&e) => return Ok(()),
                Err(e) => return Err(e),
            }
            let Some(front) = self.queue.front_mut() else {
                return Ok(());
            };
            let first = front.pos == 0;
            let payload = front.next_frame(FRAME_SIZE);
            let fin = front.pos == front.len();
            if fin {
                self.queue.pop_front();
            }
            self.queued -= payload.len();
            self.unflushed = payload.len();
            let opcode = OpCode::Data(if first { Data::Binary } else { Data::Continue });
            match self
                .ws
                .write(WsMessage::Frame(Frame::message(payload, opcode, fin)))
            {
                // Buffered: tungstenite writes it out on the next flush.
                Err(e) if would_block(&e) => return Ok(()),
                Err(e) => return Err(e),
                Ok(()) => {}
            }
        }
    }
}

fn would_block(e: &WsError) -> bool {
    matches!(e, WsError::Io(err) if err.kind() == io::ErrorKind::WouldBlock)
}

fn link_err(e: WsError) -> SyncError {
    match e {
        WsError::ConnectionClosed
        | WsError::AlreadyClosed
        // The peer closed while we still had frames to send.
        | WsError::Protocol(ProtocolError::SendAfterClosing) => SyncError::Closed,
        WsError::Io(err)
            if matches!(
                err.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::UnexpectedEof
            ) =>
        {
            SyncError::Closed
        }
        // Received a frame or message over this side's limit (both limits are
        // the same: a frame is checked on its header, a message as its frames
        // arrive, so `size` may be only what arrived so far). The sender
        // allows larger messages: other `HubLimits`, an older build, or a
        // read-only link on the hub (`set_receive_limit`).
        WsError::Capacity(CapacityError::MessageTooLong { size, max_size }) => {
            SyncError::Hub(format!(
                "received a message of at least {size} bytes, over the limit of \
                 {max_size} on this side (the sender allows larger messages)"
            ))
        }
        other => hub_err(other),
    }
}

impl Transport for HubTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let (head, data) = msg.into_encoded_parts();
        let size = head.len() + data.len();
        // The peer (same limits) would refuse it and drop the link; refuse it
        // here instead, before anything is queued.
        if size > self.limits.max_message_size {
            return Err(SyncError::MessageTooLarge {
                size,
                max: self.limits.max_message_size,
            });
        }
        let backlog = self.backlog();
        if backlog.saturating_add(size) > self.limits.write_buffer_cap() {
            return Err(SyncError::Hub(format!(
                "write buffer full: the peer is not reading ({backlog} bytes waiting, \
                 limit {})",
                self.limits.write_buffer_cap()
            )));
        }
        self.queued += size;
        self.queue.push_back(Outgoing {
            parts: [head.into(), data.into()],
            pos: 0,
        });
        self.write_out().map_err(link_err)
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        // Keep writing. A failed write does not hide what the peer sent before
        // it went away: read that first, report the error once nothing is left.
        let written = self.write_out();
        loop {
            match self.ws.read() {
                // The payload is not copied when the buffer is ours alone: a
                // message assembled from frames (every large one from this
                // build). A single frame shares tungstenite's read buffer and
                // is copied once.
                Ok(WsMessage::Binary(bytes)) => match Message::decode_owned(bytes.into()) {
                    Ok(m) => return Ok(Some(m)),
                    // Newer message types and garbage are skipped.
                    Err(_) => continue,
                },
                Ok(WsMessage::Close(_)) => return Err(SyncError::Closed),
                // Pings are answered by tungstenite on the next flush.
                Ok(_) => continue,
                Err(e) if would_block(&e) => return written.map(|()| None).map_err(link_err),
                Err(e) => return Err(link_err(e)),
            }
        }
    }

    fn backlog(&self) -> usize {
        self.queued + self.unflushed
    }

    fn max_message_size(&self) -> Option<usize> {
        Some(self.limits.max_message_size)
    }
}

impl Drop for HubTransport {
    fn drop(&mut self) {
        // What is still queued is dropped; the peer's next `Hello` after a
        // reconnect asks for it again.
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}
