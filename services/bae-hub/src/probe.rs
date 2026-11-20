//! A minimal blocking HTTP/1.1 GET, for `bae-hub healthcheck` (the Docker image
//! has no curl) and tests.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Status code and body of a response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// `GET path` on `addr` with an optional bearer token.
pub fn get(
    addr: impl ToSocketAddrs,
    path: &str,
    token: Option<&str>,
    timeout: Duration,
) -> std::io::Result<Response> {
    let addr: SocketAddr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("no address"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let auth = token.map_or_else(String::new, |t| format!("Authorization: Bearer {t}\r\n"));
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}Connection: close\r\n\r\n"
    )?;
    let mut raw = Vec::new();
    stream.take(1 << 20).read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| std::io::Error::other("malformed HTTP response"))?;
    let chunked = head.lines().any(|l| {
        l.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    Ok(Response {
        status,
        body: if chunked {
            dechunk(body)
        } else {
            body.to_owned()
        },
    })
}

fn dechunk(mut s: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = s.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if n == 0 || rest.len() < n {
            break;
        }
        out.push_str(&rest[..n]);
        s = rest[n..].trim_start_matches("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn dechunk() {
        assert_eq!(super::dechunk("3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"), "abcde");
    }
}
