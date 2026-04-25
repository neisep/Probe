use std::collections::HashMap;
use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::form_urlencoded;

use super::OAuthError;

pub struct LoopbackListener {
    listener: TcpListener,
    port: u16,
}

impl LoopbackListener {
    pub async fn bind() -> Result<Self, OAuthError> {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("valid socket addr");
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| OAuthError::Browser(format!("bind failed: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| OAuthError::Browser(format!("local_addr failed: {e}")))?
            .port();
        Ok(Self { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn redirect_uri(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    pub async fn accept_once(self) -> Result<HashMap<String, String>, OAuthError> {
        let (mut stream, _) = self
            .listener
            .accept()
            .await
            .map_err(|e| OAuthError::Browser(format!("accept failed: {e}")))?;

        const MAX_REQUEST_SIZE: usize = 64 * 1024;
        let mut buf = vec![0u8; 4096];
        let mut total = 0usize;
        loop {
            let n = stream
                .read(&mut buf[total..])
                .await
                .map_err(|e| OAuthError::Browser(format!("read failed: {e}")))?;
            if n == 0 {
                break;
            }
            total += n;
            if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if total == buf.len() {
                if buf.len() >= MAX_REQUEST_SIZE {
                    return Err(OAuthError::Browser("redirect request too large".into()));
                }
                buf.resize((buf.len() * 2).min(MAX_REQUEST_SIZE), 0);
            }
        }

        let head = std::str::from_utf8(&buf[..total])
            .map_err(|e| OAuthError::Parse(format!("non-utf8 request: {e}")))?;
        let first_line = head
            .lines()
            .next()
            .ok_or_else(|| OAuthError::Parse("empty HTTP request".into()))?;

        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(OAuthError::Parse(format!(
                "bad request line: {first_line}"
            )));
        }
        let path_and_query = parts[1];
        let query = path_and_query
            .split_once('?')
            .map(|(_, q)| q)
            .unwrap_or("");

        let params: HashMap<String, String> = form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        let body = "<!doctype html><html><body style=\"font-family:sans-serif;padding:2rem\">\
            <h2>Probe: authentication complete</h2>\
            <p>You can close this tab.</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n{}",
            body.len(),
            body
        );
        if let Err(e) = stream.write_all(response.as_bytes()).await {
            tracing::warn!("failed to write OAuth callback response: {e}");
        }
        if let Err(e) = stream.shutdown().await {
            tracing::warn!("failed to shut down OAuth callback stream: {e}");
        }

        Ok(params)
    }
}

pub fn open_url(url: &str) -> Result<(), OAuthError> {
    webbrowser::open(url)
        .map(|_| ())
        .map_err(|e| OAuthError::Browser(format!("open failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn loopback_parses_query_and_writes_200() {
        let listener = LoopbackListener::bind().await.unwrap();
        let port = listener.port();
        assert!(listener.redirect_uri("/cb").starts_with("http://127.0.0.1:"));

        let server = tokio::spawn(async move { listener.accept_once().await });

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = b"GET /callback?code=abc&state=xyz&extra=hello%20world HTTP/1.1\r\n\
                        Host: 127.0.0.1\r\n\
                        \r\n";
        stream.write_all(request).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = std::str::from_utf8(&response).unwrap();
        assert!(response_str.starts_with("HTTP/1.1 200"));
        assert!(response_str.contains("authentication complete"));

        let params = server.await.unwrap().unwrap();
        assert_eq!(params.get("code").map(String::as_str), Some("abc"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz"));
        assert_eq!(
            params.get("extra").map(String::as_str),
            Some("hello world")
        );
    }

    #[tokio::test]
    async fn loopback_handles_empty_query() {
        let listener = LoopbackListener::bind().await.unwrap();
        let port = listener.port();
        let server = tokio::spawn(async move { listener.accept_once().await });

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(b"GET /callback HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        let params = server.await.unwrap().unwrap();
        assert!(params.is_empty());
    }
}
