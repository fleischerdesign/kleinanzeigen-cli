//! HTTP transport boundary for the mobile API.
//!
//! The provider only sees a request value and a response value. Tests substitute
//! a deterministic transport; the production implementation uses `reqwest` with
//! rustls and no redirects, so the app credential cannot follow a redirect to a
//! different host.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use reqwest::{Client, Method, StatusCode};
use url::Url;

use crate::error::{Error, Result};

/// Maximum accepted provider response size.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// One HTTP request.
pub(crate) struct HttpRequest {
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
}

/// One HTTP response.
pub(crate) struct HttpResponse {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
}

/// A transport that turns a request into a response.
#[async_trait]
pub(crate) trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse>;
}

/// Production transport.
pub(crate) struct ReqwestTransport {
    client: Client,
    max_response_bytes: usize,
}

impl ReqwestTransport {
    /// Builds a client with bounded timeouts and no redirects.
    pub(crate) fn new() -> Result<Self> {
        Self::with_max_response_bytes(MAX_RESPONSE_BYTES)
    }

    fn with_max_response_bytes(max_response_bytes: usize) -> Result<Self> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| Error::Internal(format!("could not build HTTP client: {error}")))?;

        Ok(Self {
            client,
            max_response_bytes: max_response_bytes.max(1),
        })
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let mut response = self
            .client
            .request(request.method, request.url)
            .headers(request.headers)
            .send()
            .await
            .map_err(|error| Error::temporary(format!("provider request failed: {error}")))?;

        if response
            .content_length()
            .is_some_and(|length| length > self.max_response_bytes as u64)
        {
            return Err(response_too_large());
        }

        let status = response.status();
        let headers = response.headers().clone();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            Error::temporary(format!("could not read provider response: {error}"))
        })? {
            if body.len().saturating_add(chunk.len()) > self.max_response_bytes {
                return Err(response_too_large());
            }
            body.extend_from_slice(&chunk);
        }

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn response_too_large() -> Error {
    Error::Format("provider response is larger than the supported limit".to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    fn serve_chunked(body: Vec<u8>) -> (Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept request");
            let mut stream = stream;
            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request line");
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).expect("read header");
                if header == "\r\n" || header.is_empty() {
                    break;
                }
            }

            if write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n\
                 Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            )
            .is_err()
            {
                return;
            }
            for chunk in body.chunks(7) {
                if write!(stream, "{:x}\r\n", chunk.len()).is_err()
                    || stream.write_all(chunk).is_err()
                    || stream.write_all(b"\r\n").is_err()
                    || stream.flush().is_err()
                {
                    return;
                }
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        });

        (
            Url::parse(&format!("http://{address}")).expect("test server URL"),
            handle,
        )
    }

    fn request(url: Url) -> HttpRequest {
        HttpRequest {
            method: Method::GET,
            url,
            headers: HeaderMap::new(),
        }
    }

    #[tokio::test]
    async fn rejects_an_oversized_chunked_response() {
        let (url, server) = serve_chunked(vec![b'x'; 32]);
        let transport = ReqwestTransport::with_max_response_bytes(8).expect("transport");
        let error = match transport.send(request(url)).await {
            Ok(_) => panic!("oversized response must fail"),
            Err(error) => error,
        };
        server.join().expect("server finishes");
        assert!(
            error
                .to_string()
                .contains("larger than the supported limit"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn accepts_a_chunked_response_within_the_limit() {
        let (url, server) = serve_chunked(b"hello world".to_vec());
        let transport = ReqwestTransport::with_max_response_bytes(64).expect("transport");
        let response = transport.send(request(url)).await.expect("response");
        server.join().expect("server finishes");

        assert_eq!(response.body, b"hello world");
    }
}
