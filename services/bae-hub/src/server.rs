//! The listening socket (`services/bae-hub/README.md` lists the routes).
//!
//! Each incoming connection gets a short-lived thread that peeks at the HTTP
//! request head without consuming it. Plain HTTP requests (health, info, status,
//! project list) are answered there. A WebSocket request for
//! `/projects/<id>` is authenticated and routed here, then the untouched socket
//! goes to `bae_sync::HubTransport::accept` for the handshake (the same
//! WebSocket code the clients use) and the connection is attached to the
//! project's relay.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use bae_sync::HubTransport;
use serde_json::json;

use crate::auth::{Auth, AuthError, Grant, token_from_request};
use crate::config::{Access, HubConfig};
use crate::error::HubError;
use crate::link::PeerLink;
use crate::project::{PeerInfo, ProjectId};
use crate::registry::Registry;

/// Path prefix of the WebSocket sync endpoint: `/projects/<id>`.
pub const SYNC_PREFIX: &str = "/projects/";

/// Request heads larger than this are refused.
const MAX_HEAD: usize = 16 * 1024;
/// Time a client has to send its request head.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// Connections being set up at once; more are dropped (cheap flood guard).
const MAX_PENDING: usize = 128;

pub(crate) struct Shared {
    pub config: Arc<HubConfig>,
    pub auth: Auth,
    pub registry: Registry,
    pub stopping: AtomicBool,
    pub started: Instant,
    /// Connections whose thread is still running (handshake or HTTP answer).
    pub pending: AtomicUsize,
}

struct PendingGuard<'a>(&'a AtomicUsize);

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Accept connections until `shared.stopping` is set (the stopper then makes
/// one connection to wake this loop).
pub(crate) fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    for conn in listener.incoming() {
        if shared.stopping.load(Ordering::Acquire) {
            break;
        }
        let stream = match conn {
            Ok(s) => s,
            Err(e) => {
                // E.g. out of file descriptors: back off instead of spinning.
                tracing::warn!(error = %e, "accept failed");
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        if shared.pending.fetch_add(1, Ordering::AcqRel) >= MAX_PENDING {
            shared.pending.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        let shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("hub-conn".into())
            .spawn(move || {
                let _guard = PendingGuard(&shared.pending);
                if let Ok(addr) = stream.peer_addr() {
                    handle(stream, addr, &shared);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "cannot start a connection thread");
        }
    }
}

/// A parsed request head.
#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    /// Lower-case names.
    headers: Vec<(String, String)>,
    head_len: usize,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn query(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn is_websocket(&self) -> bool {
        self.header("upgrade")
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    }

    fn parse(head: &[u8]) -> Option<Request> {
        let text = std::str::from_utf8(head).ok()?;
        let mut lines = text.split("\r\n");
        let mut first = lines.next()?.split(' ');
        let method = first.next()?.to_owned();
        let target = first.next()?;
        if !first.next()?.starts_with("HTTP/1.") {
            return None;
        }
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let query = query
            .split('&')
            .filter(|kv| !kv.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                (percent_decode(k), percent_decode(v))
            })
            .collect();
        let headers = lines
            .take_while(|l| !l.is_empty())
            .filter_map(|l| {
                let (n, v) = l.split_once(':')?;
                Some((n.trim().to_ascii_lowercase(), v.trim().to_owned()))
            })
            .collect();
        Some(Request {
            method,
            path: percent_decode(path),
            query,
            headers,
            head_len: head.len(),
        })
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match b[i] {
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Peek until the whole request head (up to the blank line) has arrived.
fn peek_head(stream: &TcpStream) -> io::Result<Option<Vec<u8>>> {
    stream.set_read_timeout(Some(HEAD_TIMEOUT))?;
    let deadline = Instant::now() + HEAD_TIMEOUT;
    let mut buf = vec![0u8; MAX_HEAD];
    loop {
        let n = stream.peek(&mut buf)?;
        if n == 0 {
            return Ok(None);
        }
        if let Some(end) = buf[..n].windows(4).position(|w| w == b"\r\n\r\n") {
            buf.truncate(end + 4);
            return Ok(Some(buf));
        }
        if n == buf.len() {
            return Err(io::Error::other("request head too large"));
        }
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        // Part of the head is here; wait for the rest.
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// An HTTP response: status, body (JSON or text).
struct Reply {
    status: u16,
    body: String,
    json: bool,
}

impl Reply {
    fn json(status: u16, value: serde_json::Value) -> Reply {
        Reply {
            status,
            body: value.to_string(),
            json: true,
        }
    }

    fn error(status: u16, message: impl Into<String>) -> Reply {
        Reply::json(status, json!({ "error": message.into() }))
    }

    fn text(status: u16, body: &str) -> Reply {
        Reply {
            status,
            body: body.to_owned(),
            json: false,
        }
    }

    fn write(&self, stream: &mut TcpStream) -> io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            426 => "Upgrade Required",
            500 => "Internal Server Error",
            _ => "Service Unavailable",
        };
        let mut head = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
            self.status,
            if self.json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            },
            self.body.len()
        );
        if self.status == 401 {
            head.push_str("WWW-Authenticate: Bearer realm=\"bae-hub\"\r\n");
        }
        if self.status == 426 {
            head.push_str("Upgrade: websocket\r\n");
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes())?;
        stream.write_all(self.body.as_bytes())?;
        stream.flush()
    }
}

impl From<HubError> for Reply {
    fn from(e: HubError) -> Self {
        let status = match &e {
            HubError::InvalidProjectId(_) => 400,
            HubError::NoSuchProject(_) => 404,
            HubError::ProjectClosed(_) | HubError::DataDirInUse(_) => 503,
            _ => 500,
        };
        if status == 500 {
            tracing::error!(error = %e, "request failed");
        }
        Reply::error(status, e.to_string())
    }
}

fn handle(stream: TcpStream, addr: SocketAddr, shared: &Shared) {
    let _ = stream.set_nodelay(true);
    let head = match peek_head(&stream) {
        Ok(Some(h)) => h,
        Ok(None) => return,
        Err(e) => {
            tracing::debug!(%addr, error = %e, "no request");
            return;
        }
    };
    let Some(req) = Request::parse(&head) else {
        return answer(stream, head.len(), Reply::error(400, "malformed request"));
    };
    if req.method != "GET" {
        return answer(stream, req.head_len, Reply::error(405, "only GET"));
    }
    if let Some(id) = req.path.strip_prefix(SYNC_PREFIX) {
        let id = id.trim_end_matches('/').to_owned();
        match sync(stream, addr, &req, &id, shared) {
            Ok(()) => {}
            Err((stream, reply)) => answer(stream, req.head_len, reply),
        }
        return;
    }
    let reply = match req.path.as_str() {
        "/healthz" => Reply::text(200, "ok\n"),
        "/v1/info" => info(shared),
        "/v1/status" => status(&req, shared),
        "/v1/projects" => projects(&req, shared),
        _ => Reply::error(404, "no such endpoint"),
    };
    answer(stream, req.head_len, reply);
}

/// Consume the request head, write `reply`, close.
fn answer(mut stream: TcpStream, head_len: usize, reply: Reply) {
    let mut head = vec![0u8; head_len];
    // Read what was only peeked, so closing does not reset the connection
    // before the client has read the reply.
    let _ = stream.read_exact(&mut head);
    let _ = reply.write(&mut stream);
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

fn authenticate(req: &Request, shared: &Shared) -> Result<Grant, Reply> {
    let token = token_from_request(req.header("authorization"), req.query("token"));
    shared.auth.authenticate(token).map_err(|e| match e {
        AuthError::Missing => Reply::error(401, "token required"),
        AuthError::Invalid => Reply::error(401, "invalid token"),
    })
}

/// Public: what this server is, so clients can check compatibility before
/// asking the user for a token.
fn info(shared: &Shared) -> Reply {
    Reply::json(
        200,
        json!({
            "service": "bae-hub",
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": bae_sync::PROTOCOL_VERSION,
            "sync_path": format!("{SYNC_PREFIX}<project>"),
            "anonymous": shared.auth.anonymous().map(Access::as_str),
        }),
    )
}

fn status(req: &Request, shared: &Shared) -> Reply {
    let grant = match authenticate(req, shared) {
        Ok(g) => g,
        Err(r) => return r,
    };
    let (projects, peers) = shared.registry.counts();
    Reply::json(
        200,
        json!({
            "service": "bae-hub",
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": bae_sync::PROTOCOL_VERSION,
            "uptime_secs": shared.started.elapsed().as_secs(),
            "loaded_projects": projects,
            "peers": peers,
            "token": &*grant.name,
            "access": grant.access.as_str(),
        }),
    )
}

fn projects(req: &Request, shared: &Shared) -> Reply {
    let grant = match authenticate(req, shared) {
        Ok(g) => g,
        Err(r) => return r,
    };
    match shared.registry.list() {
        Ok(list) => {
            let visible: Vec<_> = list
                .into_iter()
                .filter(|p| {
                    ProjectId::new(&p.id)
                        .ok()
                        .is_some_and(|id| grant.access_to(&id).is_some())
                })
                .collect();
            Reply::json(200, json!({ "projects": visible }))
        }
        Err(e) => e.into(),
    }
}

/// `GET /projects/<id>` with a WebSocket upgrade. Opening a project that does
/// not exist creates it (write access only). On refusal the stream comes back
/// with the reply to send.
fn sync(
    stream: TcpStream,
    addr: SocketAddr,
    req: &Request,
    id: &str,
    shared: &Shared,
) -> Result<(), (TcpStream, Reply)> {
    if !req.is_websocket() {
        return Err((stream, Reply::error(426, "connect with a WebSocket client")));
    }
    let grant = match authenticate(req, shared) {
        Ok(g) => g,
        Err(r) => return Err((stream, r)),
    };
    let id = match ProjectId::new(id) {
        Ok(id) => id,
        Err(e) => return Err((stream, e.into())),
    };
    let Some(access) = grant.access_to(&id) else {
        let msg = format!("token {:?} may not open project {id}", &*grant.name);
        return Err((stream, Reply::error(403, msg)));
    };
    if shared.stopping.load(Ordering::Acquire) {
        return Err((stream, Reply::error(503, "hub is shutting down")));
    }
    let project = match shared.registry.open(&id, access == Access::Write) {
        Ok(p) => p,
        Err(e) => return Err((stream, e.into())),
    };
    // Authentication is done above (several tokens, per project), so the
    // handshake itself checks nothing more. Read-only connections get a small
    // incoming limit: a larger message fails as soon as its frame header
    // arrives, before the hub buffers it.
    let limits = shared.config.client_limits(access);
    let transport = match HubTransport::accept_with(stream, None, limits) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(%addr, error = %e, "WebSocket handshake failed");
            return Ok(());
        }
    };
    let link = PeerLink::new(transport, access, shared.config.peer_timeout);
    let info = PeerInfo {
        token: grant.name.clone(),
        access,
        addr,
        connected: SystemTime::now(),
    };
    if let Err(e) = project.connect(Box::new(link), info) {
        // Unloaded in the meantime; the client reconnects.
        tracing::info!(project = %project.id(), %addr, error = %e, "connection dropped");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_request_head() {
        let head = b"GET /projects/film%2Da?token=a%20b&x HTTP/1.1\r\nHost: nas\r\nUpgrade: WebSocket\r\nAuthorization: Bearer t\r\n\r\n";
        let r = Request::parse(head).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.path, "/projects/film-a");
        assert_eq!(r.query("token"), Some("a b"));
        assert_eq!(r.query("x"), Some(""));
        assert_eq!(r.header("authorization"), Some("Bearer t"));
        assert!(r.is_websocket());
        assert_eq!(r.head_len, head.len());
        assert!(Request::parse(b"garbage\r\n\r\n").is_none());
        assert!(Request::parse(b"GET / SPDY/3\r\n\r\n").is_none());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%2Fb"), "a/b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("a+b"), "a b");
    }
}
