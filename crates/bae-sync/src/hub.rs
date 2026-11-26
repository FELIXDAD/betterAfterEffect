//! Hub transport (feature `hub`, `docs/spec/sync.md` §3.1): the sync protocol over
//! WebSocket, one encoded [`Message`] per binary frame.
//!
//! The socket is non-blocking after the handshake, so `send` and `recv` never
//! wait; still, wrap it in a [`crate::ThreadedTransport`] so connecting and large
//! transfers stay off the stage thread.
//!
//! Message size and the send backlog are bounded by [`HubLimits`]; both ends of
//! a link must use the same limits.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::error::CapacityError;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode, header::AUTHORIZATION};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Error as WsError, Message as WsMessage, WebSocket};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

/// Bytes tungstenite collects before writing to the socket (its default). `send`
/// flushes after every message, so this only matters for the write buffer cap.
const WRITE_BUFFER_SIZE: usize = 128 << 10;
/// Longest WebSocket frame header: 2 bytes, 8 bytes of length, 4 bytes of mask.
const MAX_FRAME_HEADER: usize = 14;

/// Size limits of a hub connection (`docs/spec/sync.md` §3.1).
///
/// tungstenite never splits an outgoing message into several frames, so every
/// message is one frame and the frame limit is always the message limit.
///
/// Both ends of a link must use the same limits. A message larger than the
/// receiver accepts closes the link, and after reconnecting the same message is
/// sent again, forever. [`HubTransport::connect`], [`HubTransport::accept`] and
/// `services/bae-hub` (key `max_message_mib` in `hub.toml`) all default to
/// [`HubLimits::default`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubLimits {
    /// Largest message sent or accepted, in bytes (encoded [`Message`]). Sending
    /// a larger one fails; receiving one closes the link. Default
    /// [`DEFAULT_MAX_MESSAGE_SIZE`](Self::DEFAULT_MAX_MESSAGE_SIZE).
    pub max_message_size: usize,
    /// Most bytes sent but not yet taken by the socket (beyond the operating
    /// system's own buffers), i.e. the backlog of a peer that does not read. A
    /// message that does not fit makes `send` fail, so the caller (e.g.
    /// [`crate::Relay`]) drops the link instead of buffering without end.
    /// Everything not yet written counts: one pump that sends more than this to
    /// a slow peer at once (several large sublayers) drops it as well.
    ///
    /// Raised to [`min_write_buffer_size`](Self::min_write_buffer_size) when
    /// smaller, so one message of the largest size always fits; see
    /// [`write_buffer_cap`](Self::write_buffer_cap). Default `max_message_size` +
    /// [`WRITE_BUFFER_MARGIN`](Self::WRITE_BUFFER_MARGIN).
    pub max_write_buffer_size: usize,
}

impl HubLimits {
    /// Default [`max_message_size`](Self::max_message_size): 256 MiB, enough for
    /// the snapshot of a very large sublayer.
    pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 256 << 20;
    /// What the default write buffer cap allows on top of one message of the
    /// largest size: room for the small messages (hellos, presence, edits) that
    /// queue up while a large one drains to a slow peer.
    pub const WRITE_BUFFER_MARGIN: usize = 16 << 20;

    /// `max_message_size` with the default write buffer cap for it.
    pub fn with_max_message_size(max_message_size: usize) -> Self {
        HubLimits {
            max_message_size,
            max_write_buffer_size: max_message_size.saturating_add(Self::WRITE_BUFFER_MARGIN),
        }
    }

    /// Smallest write buffer cap that still lets a message of the largest size
    /// through: tungstenite needs room for its own write buffer plus one whole
    /// frame (`WebSocketConfig::max_write_buffer_size`).
    pub fn min_write_buffer_size(&self) -> usize {
        WRITE_BUFFER_SIZE
            .saturating_add(self.max_message_size)
            .saturating_add(MAX_FRAME_HEADER)
    }

    /// The write buffer cap in use: `max_write_buffer_size`, but at least
    /// [`min_write_buffer_size`](Self::min_write_buffer_size).
    pub fn write_buffer_cap(&self) -> usize {
        self.max_write_buffer_size.max(self.min_write_buffer_size())
    }

    fn ws_config(&self) -> WebSocketConfig {
        WebSocketConfig::default()
            .write_buffer_size(WRITE_BUFFER_SIZE)
            .max_write_buffer_size(self.write_buffer_cap())
            .max_message_size(Some(self.max_message_size))
            .max_frame_size(Some(self.max_message_size))
    }
}

impl Default for HubLimits {
    fn default() -> Self {
        Self::with_max_message_size(Self::DEFAULT_MAX_MESSAGE_SIZE)
    }
}

/// One WebSocket connection carrying the sync protocol, client or server side.
pub struct HubTransport {
    ws: WebSocket<TcpStream>,
    limits: HubLimits,
}

impl std::fmt::Debug for HubTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubTransport")
            .field("peer", &self.ws.get_ref().peer_addr().ok())
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
    /// same).
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
            tungstenite::client::client_with_config(request, stream, Some(limits.ws_config()))
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
    /// same).
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
        let ws = tungstenite::accept_hdr_with_config(stream, check, Some(limits.ws_config()))
            .map_err(hub_err)?;
        Self::ready(ws, limits)
    }

    fn ready(ws: WebSocket<TcpStream>, limits: HubLimits) -> Result<Self, SyncError> {
        let s = ws.get_ref();
        s.set_read_timeout(None).map_err(hub_err)?;
        s.set_write_timeout(None).map_err(hub_err)?;
        s.set_nonblocking(true).map_err(hub_err)?;
        Ok(HubTransport { ws, limits })
    }

    /// The size limits of this connection.
    pub fn limits(&self) -> HubLimits {
        self.limits
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
        // Received a frame over `max_message_size` (checked on its header, before
        // the payload is read).
        WsError::Capacity(CapacityError::MessageTooLong { size, max_size }) => {
            SyncError::Hub(format!(
                "received a message of {size} bytes, over the limit of {max_size} \
                 (client and hub must use the same HubLimits)"
            ))
        }
        // `max_write_buffer_size` reached: the frame was not queued.
        WsError::WriteBufferFull(_) => {
            SyncError::Hub("write buffer full: the peer is not reading".into())
        }
        other => hub_err(other),
    }
}

impl Transport for HubTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let bytes = msg.encode();
        // The peer (same limits) would refuse it and drop the link; say why here.
        if bytes.len() > self.limits.max_message_size {
            return Err(SyncError::Hub(format!(
                "message of {} bytes is over the limit of {} (HubLimits::max_message_size)",
                bytes.len(),
                self.limits.max_message_size
            )));
        }
        match self.ws.send(WsMessage::Binary(bytes.into())) {
            Ok(()) => Ok(()),
            // Queued; the rest goes out on later flushes. Only an I/O error can
            // be `WouldBlock`; `WriteBufferFull` (nothing queued) is an error.
            Err(e) if would_block(&e) => Ok(()),
            Err(e) => Err(link_err(e)),
        }
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        match self.ws.flush() {
            Err(e) if !would_block(&e) => return Err(link_err(e)),
            _ => {}
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
                Err(e) if would_block(&e) => return Ok(None),
                Err(e) => return Err(link_err(e)),
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
