//! A minimal loopback HTTP server.
//!
//! # Why hand-written rather than a framework
//!
//! This server binds `127.0.0.1`, serves three GET routes, and is consumed by exactly
//! one client. A full HTTP framework would add a large dependency tree to the trusted
//! build for routing, extractors, middleware and TLS that none of that is used here.
//! The Wave E brief asks for the minimum coherent complexity, and three routes is it.
//!
//! What it deliberately does NOT do — no keep-alive, no chunked encoding, no compression,
//! no request bodies — is what keeps it small enough to read in one sitting.

use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, warn};

/// Largest request head accepted. Nothing legitimate approaches this; it bounds what a
/// misbehaving client can make the server buffer.
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// A parsed request line. Headers are read and discarded: none of the three routes
/// varies on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    /// What followed `?` in the target, still percent-encoded. Only `/api/orb/lumen`
    /// reads it: the view it asks for is presentation, never a command.
    pub query: String,
    pub authorization: Option<String>,
}

impl Request {
    /// Parse the request line out of a raw HTTP head.
    pub fn parse(raw: &str) -> Option<Self> {
        let line = raw.lines().next()?;
        let mut parts = line.split_whitespace();
        let method = parts.next()?.to_string();
        let target = parts.next()?;
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let (path, query) = (path.to_string(), query.chars().take(4096).collect());
        // Require a real HTTP version token. Checking only that a third field exists
        // would let arbitrary whitespace-separated text ("not http at all") parse as a
        // request line and reach the router.
        let version = parts.next()?;
        if !version.starts_with("HTTP/") {
            return None;
        }
        Some(Self {
            method,
            path,
            query,
            authorization: raw
                .lines()
                .skip(1)
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
                .map(|(_, value)| value.trim().to_string()),
        })
    }
}

/// A response ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            content_type,
            body: body.into(),
        }
    }

    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self::ok("application/json; charset=utf-8", body)
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            content_type: "text/plain; charset=utf-8",
            body: b"not found\n".to_vec(),
        }
    }

    pub fn method_not_allowed() -> Self {
        Self {
            status: 405,
            content_type: "text/plain; charset=utf-8",
            body: b"the observatory is read-only; only GET is served\n".to_vec(),
        }
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "Error",
        }
    }

    /// Serialize to the wire.
    pub fn to_bytes(&self) -> Vec<u8> {
        let head = format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: {}\r\n\
             Content-Length: {}\r\n\
             Cache-Control: no-store\r\n\
             X-Content-Type-Options: nosniff\r\n\
             Connection: close\r\n\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len()
        );
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

/// Bind the observatory's HTTP endpoint.
///
/// Refuses any address that is not loopback. The observatory exposes node identity,
/// hardware inventory and module state; that is an internal view, and binding it to a
/// routable interface would publish it to the network.
pub async fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    if !addr.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing to bind {addr}: the display endpoint must be loopback-only"),
        ));
    }
    TcpListener::bind(addr).await
}

/// Serve one connection.
pub async fn serve_connection<F>(mut stream: TcpStream, handler: F) -> std::io::Result<()>
where
    F: FnOnce(Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>,
{
    let mut buffer = vec![0u8; MAX_REQUEST_BYTES];
    let mut filled = 0usize;

    // Read until the end of the request head, or until the cap is reached.
    loop {
        if filled >= buffer.len() {
            break;
        }
        let read = stream.read(&mut buffer[filled..]).await?;
        if read == 0 {
            break;
        }
        filled += read;
        if buffer[..filled].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let raw = String::from_utf8_lossy(&buffer[..filled]).to_string();
    let response = match Request::parse(&raw) {
        Some(request) if request.method == "GET" => handler(request).await,
        Some(request) => {
            debug!(method = %request.method, "rejected non-GET");
            Response::method_not_allowed()
        }
        None => Response::not_found(),
    };

    stream.write_all(&response.to_bytes()).await?;
    stream.flush().await?;
    let _ = stream.shutdown().await;
    Ok(())
}

/// Accept and serve until `shutdown` fires.
pub async fn serve<F>(
    listener: TcpListener,
    handler: F,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) where
    F: Fn(Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
        + Clone
        + Send
        + Sync
        + 'static,
{
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    return;
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _peer)) => {
                        let handler = handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = serve_connection(stream, |r| handler(r)).await {
                                debug!(error = %e, "display connection ended");
                            }
                        });
                    }
                    Err(e) => {
                        warn!(error = %e, "display accept failed");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_normal_request_line() {
        let request = Request::parse("GET /api/snapshot HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/api/snapshot");
    }

    #[test]
    fn strips_a_query_string() {
        let request = Request::parse("GET /api/snapshot?t=1 HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(request.path, "/api/snapshot");
        assert_eq!(request.query, "t=1");
    }

    #[test]
    fn rejects_malformed_request_lines() {
        for junk in ["", "GET", "GET /", "\r\n\r\n", "not http at all"] {
            assert!(
                Request::parse(junk).is_none(),
                "{junk:?} should not parse as a request"
            );
        }
    }

    #[test]
    fn a_response_carries_an_accurate_content_length() {
        let response = Response::json(br#"{"a":1}"#.to_vec());
        let wire = String::from_utf8(response.to_bytes()).unwrap();
        assert!(wire.contains("Content-Length: 7"));
        assert!(wire.contains("HTTP/1.1 200 OK"));
        assert!(wire.ends_with(r#"{"a":1}"#));
    }

    #[test]
    fn responses_forbid_caching_and_sniffing() {
        // The snapshot changes every second and must never be cached; nosniff stops a
        // JSON body being reinterpreted as script.
        let wire = String::from_utf8(Response::json(b"{}".to_vec()).to_bytes()).unwrap();
        assert!(wire.contains("Cache-Control: no-store"));
        assert!(wire.contains("X-Content-Type-Options: nosniff"));
    }

    #[tokio::test]
    async fn binding_a_routable_address_is_refused() {
        // The observatory exposes hardware inventory and node identity. Binding it to a
        // routable interface would publish that to the network.
        let err = bind("0.0.0.0:0".parse().unwrap()).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("loopback"));
    }

    #[tokio::test]
    async fn binding_loopback_succeeds() {
        let listener = bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }

    #[tokio::test]
    async fn a_get_is_served_and_a_post_is_refused() {
        let listener = bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                serve_connection(stream, |request| {
                    Box::pin(
                        async move { Response::json(format!(r#"{{"path":"{}"}}"#, request.path)) },
                    )
                })
                .await
                .ok();
            }
        });

        // GET is served.
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET /api/snapshot HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        client.read_to_string(&mut body).await.unwrap();
        assert!(body.contains("200 OK"));
        assert!(body.contains("/api/snapshot"));

        // POST is refused: the observatory is read-only.
        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"POST /api/snapshot HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        client.read_to_string(&mut body).await.unwrap();
        assert!(body.contains("405"));
        assert!(body.contains("read-only"));
    }

    #[tokio::test]
    async fn an_oversized_request_head_does_not_exhaust_memory() {
        let listener = bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            serve_connection(stream, |_| Box::pin(async { Response::not_found() }))
                .await
                .ok();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        // Far more than MAX_REQUEST_BYTES, with no terminator.
        let flood = vec![b'x'; MAX_REQUEST_BYTES * 4];
        let _ = client.write_all(&flood).await;
        let mut body = String::new();
        // The server must answer rather than buffering forever.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.read_to_string(&mut body),
        )
        .await;
        assert!(
            body.contains("HTTP/1.1"),
            "server did not respond: {body:?}"
        );
    }
}
