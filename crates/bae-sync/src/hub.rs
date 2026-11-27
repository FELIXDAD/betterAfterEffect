//! Hub transport (feature `hub`, `docs/spec/sync.md` §3.1): the sync protocol over
//! WebSocket, one encoded [`Message`] per binary frame.
//!
//! The socket is non-blocking after the handshake, so `send` and `recv` never
//! wait; still, wrap it in a [`crate::ThreadedTransport`] so connecting and large
//! transfers stay off the stage thread.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::handshake::client::Request as ClientRequest;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode, header::AUTHORIZATION};
use tungstenite::{Error as WsError, Message as WsMessage, WebSocket};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::{Access, Transport, check_size};

/// Header of the hub's handshake response that tells the client what it may
/// do: `read` or `write` ([`HubTransport::access`]). Hubs that do not send it
/// leave the client at [`Access::Unknown`].
pub const ACCESS_HEADER: &str = "x-bae-access";

/// Header of both handshake messages (the client's request, the hub's
/// response) that tells the other side the largest message this side takes
/// (its [`HubLimits::max_incoming`], in bytes). Each side then sends at most
/// the smaller of its own `max_message` and that
/// ([`HubTransport::max_message_size`]): a message the other side would drop
/// the link over fails to send instead, and only its document is skipped
/// (a hub configured for smaller messages, the hub's smaller limit for
/// read-only connections). Peers that do not send it (older builds) are sent
/// up to `max_message`, as before.
pub const MAX_INCOMING_HEADER: &str = "x-bae-max-incoming";

/// What a [`MAX_INCOMING_HEADER`] says: a positive number of bytes, else
/// nothing (missing or garbled: the peer's limit is not known).
fn max_incoming(headers: &tungstenite::http::HeaderMap) -> Option<usize> {
    let n: u64 = headers
        .get(MAX_INCOMING_HEADER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (n > 0).then(|| usize::try_from(n).unwrap_or(usize::MAX))
}

fn max_incoming_value(limits: &HubLimits) -> HeaderValue {
    HeaderValue::from(limits.max_incoming.max(1) as u64)
}

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
/// more than `max_unsent` never waits (as long as it is not below
/// `max_message`: a message that finds nothing waiting is always taken): the
/// link fails instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HubLimits {
    /// Largest encoded protocol message this side sends. A whole sublayer
    /// snapshot must fit: a larger one fails to send
    /// ([`SyncError::MessageTooLarge`]) instead of being refused remotely,
    /// and the session skips only its document. When the peer said how much
    /// it takes in the handshake ([`MAX_INCOMING_HEADER`]), the smaller of the
    /// two applies ([`HubTransport::max_message_size`]); a peer that does not
    /// say (an older build) must take at least this much, or it drops the link
    /// over a larger message.
    pub max_message: usize,
    /// Largest message accepted from the peer (also the frame limit, since a
    /// message is one frame): a larger one fails `recv` as soon as its frame
    /// header arrives, before it is buffered, and the link is dropped. Told to
    /// the peer in the handshake ([`MAX_INCOMING_HEADER`]), so peers of this
    /// version do not send larger ones. The hub takes less from read-only
    /// connections (16 MiB by default), whose sessions should send no
    /// document data.
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
    /// stay below it if it is at least `max_message + max_backlog` (better
    /// [`HubLimits::max_unsent_for`], the default); it stops what does not
    /// hold back (snapshot requests sent over and over, a session without
    /// flow control).
    pub max_unsent: usize,
    /// How long nothing may go out while more than `max_backlog` waits; also
    /// how long [`Transport::close`] waits while nothing moves.
    pub stall_timeout: Duration,
}

impl HubLimits {
    /// The `max_unsent` that fits `max_message` and `max_backlog`, as the
    /// default is made: room for what a held-back session leaves waiting (one
    /// message over `max_backlog`) plus one snapshot answer. It must be at
    /// least `max_message + max_backlog`, or a peer that a session held back
    /// for legitimately can be cut off.
    pub fn max_unsent_for(max_message: usize, max_backlog: usize) -> usize {
        max_message.saturating_mul(2).saturating_add(max_backlog)
    }
}

impl Default for HubLimits {
    /// 256 MiB messages both ways, flow control at 16 MiB, at most 528 MiB
    /// waiting ([`HubLimits::max_unsent_for`]), 30 s stall timeout.
    fn default() -> Self {
        const MAX_MESSAGE: usize = 256 << 20;
        HubLimits {
            max_message: MAX_MESSAGE,
            max_incoming: MAX_MESSAGE,
            max_backlog: crate::session::DEFAULT_MAX_BACKLOG,
            max_unsent: Self::max_unsent_for(MAX_MESSAGE, crate::session::DEFAULT_MAX_BACKLOG),
            stall_timeout: Duration::from_secs(30),
        }
    }
}

/// The socket, counting the bytes written to it and read from it: how
/// [`HubTransport`] tells a slow peer (bytes keep leaving) from one that
/// stopped reading, and a peer sending a large message (bytes keep arriving)
/// from one that went silent.
#[derive(Debug)]
struct Counted {
    stream: TcpStream,
    written: u64,
    read: u64,
}

impl Counted {
    fn new(stream: TcpStream) -> Self {
        Counted {
            stream,
            written: 0,
            read: 0,
        }
    }
}

impl Read for Counted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.stream.read(buf)?;
        self.read += n as u64;
        Ok(n)
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
    /// Client side: what the hub said we may do. Server side: what we said.
    access: Access,
    /// The largest message the peer said it takes ([`MAX_INCOMING_HEADER`]).
    peer_max_incoming: Option<usize>,
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
    /// [`set_idle_timeout`](HubTransport::set_idle_timeout).
    idle_timeout: Option<Duration>,
    /// `Counted::read` when we last looked, and when it last changed.
    read: u64,
    heard: Instant,
    /// The idle timeout was reported: the link is dead, `recv` says `Closed`.
    silent: bool,
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

/// Connect to the first of `addrs` that answers, IPv4 first (the hub listens
/// on `0.0.0.0` by default, and a name like `localhost` or `nas.local` often
/// resolves to an IPv6 address first). Each address gets an equal share of
/// what is left of `timeout`, so one that drops packets does not use up the
/// time of the others.
fn connect_any(addrs: &[SocketAddr], timeout: Duration) -> io::Result<TcpStream> {
    let mut unique: Vec<SocketAddr> = Vec::with_capacity(addrs.len());
    for a in addrs {
        if !unique.contains(a) {
            unique.push(*a);
        }
    }
    let mut addrs = unique;
    addrs.sort_by_key(SocketAddr::is_ipv6);
    let deadline = Instant::now() + timeout;
    let mut last = None;
    for (i, addr) in addrs.iter().enumerate() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let share = left / u32::try_from(addrs.len() - i).unwrap_or(u32::MAX);
        match TcpStream::connect_timeout(addr, share.max(Duration::from_millis(1))) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "connect timed out")))
}

/// The handshake request for `url`, and the host and port to connect to.
/// Checks everything that does not need the network.
fn client_request(
    url: &str,
    token: Option<&str>,
) -> Result<(ClientRequest, String, u16), SyncError> {
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
    Ok((request, host, port))
}

/// Whether `url` and `token` can be used for [`HubTransport::connect`] at all
/// (scheme, host, header syntax), without touching the network.
pub(crate) fn check_url(url: &str, token: Option<&str>) -> Result<(), SyncError> {
    client_request(url, token).map(|_| ())
}

/// The longest `GET /v1/status` reply [`status_access`] reads.
const STATUS_MAX: usize = 64 << 10;

/// What the hub's `GET /v1/status` says `token` may do (its `"access"`): the
/// fallback for hubs that do not send [`ACCESS_HEADER`] in the handshake
/// (bae-hub 0.0.1), whose read-only tokens would otherwise only show by edits
/// being dropped and sent again. Same host, port, token and `?token=` query
/// as the sync `url`. [`Access::Unknown`] if it does not say or anything
/// fails. Blocks for at most about `timeout` per step (connect, write, each
/// read): call it on a connect thread.
pub(crate) fn status_access(url: &str, token: Option<&str>, timeout: Duration) -> Access {
    status_reply(url, token, timeout)
        .and_then(|v| Some(Access::parse(v.get("access")?.as_str()?)))
        .unwrap_or(Access::Unknown)
}

/// The JSON body of a `200` reply to `GET /v1/status` on the hub of `url`.
fn status_reply(url: &str, token: Option<&str>, timeout: Duration) -> Option<serde_json::Value> {
    let (request, host, port) = client_request(url, token).ok()?;
    let path = match request.uri().query() {
        Some(q) => format!("/v1/status?{q}"),
        None => "/v1/status".to_owned(),
    };
    let addrs: Vec<SocketAddr> = (host.as_str(), port).to_socket_addrs().ok()?.collect();
    let mut stream = connect_any(&addrs, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut head = format!(
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAccept: application/json\r\nConnection: close\r\n"
    );
    // Checked by `client_request`: no line breaks in it.
    if let Some(auth) = request.headers().get(AUTHORIZATION) {
        head.push_str(&format!("Authorization: {}\r\n", auth.to_str().ok()?));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head_len, body_len) = loop {
        let end = buf.windows(4).position(|w| w == b"\r\n\r\n");
        if let Some(at) = end {
            let len = content_length(&buf[..at]);
            if len.is_some_and(|n| buf.len() >= at + 4 + n) {
                break (at, len);
            }
        }
        if buf.len() > STATUS_MAX {
            return None;
        }
        match stream.read(&mut chunk) {
            // Closed: the body is what came (no `Content-Length`).
            Ok(0) => break (end?, None),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    };
    let head = std::str::from_utf8(&buf[..head_len]).ok()?;
    if head.split_whitespace().nth(1) != Some("200") {
        return None;
    }
    let body = &buf[head_len + 4..];
    let body = body_len.map_or(body, |n| &body[..n]);
    serde_json::from_slice(body).ok()
}

/// The `Content-Length` of an HTTP response head.
fn content_length(head: &[u8]) -> Option<usize> {
    std::str::from_utf8(head).ok()?.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse().ok())?
    })
}

/// A refused handshake: the status, and the hub's `{"error": ...}` text if
/// the body came with the response head.
fn refused(resp: &tungstenite::http::Response<Option<Vec<u8>>>) -> SyncError {
    let status = resp.status();
    let message = resp
        .body()
        .as_deref()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
        .and_then(|v| v.get("error")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| status.canonical_reason().unwrap_or_default().to_owned());
    SyncError::HubRefused {
        status: status.as_u16(),
        message,
    }
}

impl HubTransport {
    /// Connect to a hub at `url` (`ws://host:port/path`; the path and query
    /// select the project, as defined by the hub). `token` is sent as
    /// `Authorization: Bearer <token>`. Blocks for at most about `timeout` per
    /// step (connect, handshake); every address the host name resolves to is
    /// tried, IPv4 first. A hub that answers with an HTTP error gives
    /// [`SyncError::HubRefused`]; what the hub allows is in
    /// [`access`](Self::access).
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
        let (mut request, host, port) = client_request(url, token)?;
        request
            .headers_mut()
            .insert(MAX_INCOMING_HEADER, max_incoming_value(&limits));
        let addrs: Vec<SocketAddr> = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(hub_err)?
            .collect();
        if addrs.is_empty() {
            return Err(SyncError::Hub(format!("cannot resolve {host}")));
        }
        let stream = connect_any(&addrs, timeout).map_err(hub_err)?;
        stream.set_nodelay(true).map_err(hub_err)?;
        stream.set_read_timeout(Some(timeout)).map_err(hub_err)?;
        stream.set_write_timeout(Some(timeout)).map_err(hub_err)?;
        let (ws, response) =
            tungstenite::client::client(request, Counted::new(stream)).map_err(|e| match e {
                tungstenite::HandshakeError::Failure(WsError::Http(resp)) => refused(&resp),
                other => hub_err(other),
            })?;
        let access = response
            .headers()
            .get(ACCESS_HEADER)
            .and_then(|v| v.to_str().ok())
            .map_or(Access::Unknown, Access::parse);
        let peer_max_incoming = max_incoming(response.headers());
        Self::ready(ws, limits, access, peer_max_incoming)
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
        Self::accept_granting(stream, token, limits, Access::Unknown)
    }

    /// [`accept_with`](Self::accept_with), telling the client what it may do
    /// in the handshake response ([`ACCESS_HEADER`]; nothing for
    /// [`Access::Unknown`]). The hub checks access itself: a read-only client
    /// that sends edits anyway gets them dropped. Every `accept` also tells
    /// the client `limits.max_incoming` ([`MAX_INCOMING_HEADER`]) and sends
    /// at most what the client said it takes.
    pub fn accept_granting(
        stream: TcpStream,
        token: Option<&str>,
        limits: HubLimits,
        access: Access,
    ) -> Result<Self, SyncError> {
        stream.set_nonblocking(false).map_err(hub_err)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(hub_err)?;
        stream.set_nodelay(true).map_err(hub_err)?;
        let expected = token.map(|t| format!("Bearer {t}"));
        let mut peer_max_incoming = None;
        // The callback signature (and its large error type) is tungstenite's.
        #[allow(clippy::result_large_err)]
        let check = |req: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
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
                _ => {
                    if access != Access::Unknown {
                        resp.headers_mut()
                            .insert(ACCESS_HEADER, HeaderValue::from_static(access.as_str()));
                    }
                    resp.headers_mut()
                        .insert(MAX_INCOMING_HEADER, max_incoming_value(&limits));
                    peer_max_incoming = max_incoming(req.headers());
                    Ok(resp)
                }
            }
        };
        let ws = tungstenite::accept_hdr(Counted::new(stream), check).map_err(hub_err)?;
        Self::ready(ws, limits, access, peer_max_incoming)
    }

    fn ready(
        ws: WebSocket<Counted>,
        limits: HubLimits,
        access: Access,
        peer_max_incoming: Option<usize>,
    ) -> Result<Self, SyncError> {
        let s = &ws.get_ref().stream;
        s.set_read_timeout(None).map_err(hub_err)?;
        s.set_write_timeout(None).map_err(hub_err)?;
        s.set_nonblocking(true).map_err(hub_err)?;
        let written = ws.get_ref().written;
        let read = ws.get_ref().read;
        let mut transport = HubTransport {
            ws,
            limits,
            access,
            peer_max_incoming,
            unsent: 0,
            written,
            progress: Instant::now(),
            failed: None,
            over: None,
            idle_timeout: None,
            read,
            heard: Instant::now(),
            silent: false,
        };
        transport.set_limits(limits);
        Ok(transport)
    }

    /// Change the limits of an open connection. The peer was told
    /// `max_incoming` in the handshake ([`MAX_INCOMING_HEADER`]); a smaller
    /// one set here is not told, and the peer may send up to the old one.
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

    /// The largest message the peer said it takes in the handshake
    /// ([`MAX_INCOMING_HEADER`]); `None` if it did not say (an older build).
    pub fn peer_max_incoming(&self) -> Option<usize> {
        self.peer_max_incoming
    }

    /// Fail [`recv`](Transport::recv) once not a byte arrived for `timeout`
    /// (`None`, the default: never). A peer that went away without closing
    /// the connection (power loss, a dropped VPN or Wi-Fi) sends no reset, and
    /// TCP may take many minutes to notice; the hub says `Hello` every 5
    /// seconds, so a client can tell much sooner. A large message counts as
    /// news while its bytes keep arriving. Reported once; later calls of
    /// `recv` return [`SyncError::Closed`] (a [`crate::ThreadedTransport`]
    /// stops there instead of reporting it again on every tick).
    pub fn set_idle_timeout(&mut self, timeout: Option<Duration>) {
        self.idle_timeout = timeout;
        self.heard = Instant::now();
    }

    /// Fails when nothing arrived for the idle timeout.
    fn check_idle(&mut self) -> Result<(), SyncError> {
        let now = Instant::now();
        let read = self.ws.get_ref().read;
        if read != self.read {
            self.read = read;
            self.heard = now;
            return Ok(());
        }
        match self.idle_timeout {
            Some(t) if now.saturating_duration_since(self.heard) >= t => {
                self.silent = true;
                Err(SyncError::Hub(format!(
                    "nothing received for {:.1} s",
                    t.as_secs_f32()
                )))
            }
            _ => Ok(()),
        }
    }

    /// Client side: what the hub said this connection may do
    /// ([`ACCESS_HEADER`]); [`Access::Unknown`] if it did not say (older
    /// hubs). A read-only connection's edits are dropped by the hub, so the
    /// client should not send them ([`crate::SessionConfig::send_data`]).
    /// Server side: what [`accept_granting`](Self::accept_granting) announced.
    pub fn access(&self) -> Access {
        self.access
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
    /// A message over [`max_message_size`](Transport::max_message_size)
    /// fails with [`SyncError::MessageTooLarge`] before anything is encoded
    /// or queued, and the link stays usable.
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        if let Some(over) = &self.over {
            return Err(SyncError::Hub(over.clone()));
        }
        // tungstenite only checks the size of incoming messages; the peer would
        // drop the link over an oversized one, so fail here with a clear reason.
        check_size(&msg, self.max_message_size())?;
        let bytes = msg.encode();
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
        if self.silent {
            return Err(SyncError::Closed);
        }
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
                        (None, None) => self.check_idle().map(|()| None),
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

    /// [`HubLimits::max_message`], or what the peer said it takes
    /// ([`peer_max_incoming`](HubTransport::peer_max_incoming)) if that is
    /// less.
    fn max_message_size(&self) -> Option<usize> {
        let own = self.limits.max_message;
        Some(self.peer_max_incoming.map_or(own, |peer| own.min(peer)))
    }
}

impl Drop for HubTransport {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A name that resolves to an address nobody listens on first (IPv6
    /// `localhost` against a hub on `0.0.0.0`): the next address is tried.
    #[test]
    fn connect_tries_every_address() {
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let refused = closed.local_addr().unwrap();
        drop(closed);
        let live = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = live.local_addr().unwrap();
        let stream = connect_any(&[refused, refused, addr], Duration::from_secs(20)).unwrap();
        assert_eq!(stream.peer_addr().unwrap(), addr);
        assert!(connect_any(&[refused], Duration::from_secs(20)).is_err());
        assert!(connect_any(&[], Duration::from_secs(1)).is_err());
    }
}
