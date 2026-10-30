//! Minimal GET-only loopback HTTP transport for normalized world snapshots.

use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, warn};

const MAX_REQUEST_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
}

impl Request {
    fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.lines().next()?.split_whitespace();
        let method = parts.next()?.to_string();
        let target = parts.next()?;
        if !parts.next()?.starts_with("HTTP/") {
            return None;
        }
        Some(Self {
            method,
            path: target.split('?').next().unwrap_or(target).to_string(),
            authorization: raw
                .lines()
                .skip(1)
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
                .map(|(_, value)| value.trim().to_string()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body: body.into(),
        }
    }

    pub fn error(status: u16, body: &'static [u8]) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.to_vec(),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "Error",
        };
        let mut bytes = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
            self.status,
            reason,
            self.content_type,
            self.body.len()
        )
        .into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

pub async fn bind(address: SocketAddr) -> std::io::Result<TcpListener> {
    if !address.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "archie-worldd endpoint must be loopback-only",
        ));
    }
    TcpListener::bind(address).await
}

async fn serve_connection<F>(mut stream: TcpStream, handler: F) -> std::io::Result<()>
where
    F: FnOnce(Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>,
{
    let mut buffer = vec![0; MAX_REQUEST_BYTES];
    let mut filled = 0;
    loop {
        if filled == buffer.len() {
            break;
        }
        let read = stream.read(&mut buffer[filled..]).await?;
        if read == 0 {
            break;
        }
        filled += read;
        if buffer[..filled]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
        {
            break;
        }
    }
    let response = match Request::parse(&String::from_utf8_lossy(&buffer[..filled])) {
        Some(request) if request.method == "GET" => handler(request).await,
        Some(_) => Response::error(405, b"archie-worldd is read-only; only GET is served\n"),
        None => Response::error(404, b"not found\n"),
    };
    stream.write_all(&response.to_bytes()).await?;
    stream.shutdown().await
}

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
                if *shutdown.borrow() { return; }
            }
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        if let Err(error) = serve_connection(stream, |request| handler(request)).await {
                            debug!(%error, "world client connection ended");
                        }
                    });
                }
                Err(error) => {
                    warn!(%error, "world accept failed");
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn refuses_routable_bind_and_non_get_requests() {
        assert!(bind("0.0.0.0:0".parse().unwrap()).await.is_err());
        let listener = bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            serve_connection(stream, |_| Box::pin(async { Response::json(b"{}") }))
                .await
                .unwrap();
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"POST /v1/world HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        assert!(response.contains("405 Method Not Allowed"));
    }
}
