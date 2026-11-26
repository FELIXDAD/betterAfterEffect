//! Hub transport (feature `hub`, `docs/spec/sync.md` §3.1): the sync protocol over
//! WebSocket, one encoded [`Message`] per binary frame.
//!
//! The socket is non-blocking after the handshake, so `send` and `recv` never
//! wait; still, wrap it in a [`crate::ThreadedTransport`] so connecting and large
//! transfers stay off the stage thread.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode, header::AUTHORIZATION};
use tungstenite::{Error as WsError, Message as WsMessage, WebSocket};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

/// Size limits of a hub connection, the same in both directions.
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
    /// must fit: a larger one fails to send instead of being refused remotely.
    pub max_message: usize,
    /// Bytes that may wait for a peer that is not reading, on top of room for
    /// one more message of up to `max_message`. Past that, `send` fails and the
    /// connection is dropped (a relay drops it at once) instead of growing
    /// without bound.
    pub max_backlog: usize,
}

impl Default for HubLimits {
    fn default() -> Self {
        HubLimits {
            max_message: 256 << 20,
            max_backlog: 16 << 20,
        }
    }
}

/// One WebSocket connection carrying the sync protocol, client or server side.
pub struct HubTransport {
    ws: WebSocket<TcpStream>,
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
        let (ws, _) = tungstenite::client::client(request, stream).map_err(|e| match e {
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
        let ws = tungstenite::accept_hdr(stream, check).map_err(hub_err)?;
        Self::ready(ws, limits)
    }

    fn ready(ws: WebSocket<TcpStream>, limits: HubLimits) -> Result<Self, SyncError> {
        let s = ws.get_ref();
        s.set_read_timeout(None).map_err(hub_err)?;
        s.set_write_timeout(None).map_err(hub_err)?;
        s.set_nonblocking(true).map_err(hub_err)?;
        let mut transport = HubTransport { ws };
        transport.set_limits(limits);
        Ok(transport)
    }

    /// Change the size limits of an open connection.
    pub fn set_limits(&mut self, limits: HubLimits) {
        let max_message = limits.max_message.max(1);
        self.ws.set_config(|c| {
            c.max_message_size = Some(max_message);
            c.max_frame_size = Some(max_message);
            c.max_write_buffer_size = c
                .write_buffer_size
                .saturating_add(max_message)
                .saturating_add(limits.max_backlog.max(1));
        });
    }

    /// The connection's size limits.
    pub fn limits(&self) -> HubLimits {
        let c = self.ws.get_config();
        let max_message = c.max_message_size.unwrap_or(usize::MAX);
        HubLimits {
            max_message,
            max_backlog: c
                .max_write_buffer_size
                .saturating_sub(c.write_buffer_size)
                .saturating_sub(max_message),
        }
    }
}

fn would_block(e: &WsError) -> bool {
    matches!(e, WsError::Io(io) if io.kind() == std::io::ErrorKind::WouldBlock)
}

fn link_err(e: WsError) -> SyncError {
    match e {
        WsError::ConnectionClosed | WsError::AlreadyClosed => SyncError::Closed,
        WsError::WriteBufferFull(_) => {
            SyncError::Hub("peer is not reading: send queue is full".into())
        }
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
        match self.ws.send(WsMessage::Binary(bytes.into())) {
            // Queued; the rest goes out on later flushes.
            Err(e) if would_block(&e) => Ok(()),
            r => r.map_err(link_err),
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
