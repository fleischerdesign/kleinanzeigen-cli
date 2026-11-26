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
}

impl ReqwestTransport {
    /// Builds a client with bounded timeouts and no redirects.
    pub(crate) fn new() -> Result<Self> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| Error::Internal(format!("could not build HTTP client: {error}")))?;

        Ok(Self { client })
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
        let response = self
            .client
            .request(request.method, request.url)
            .headers(request.headers)
            .send()
            .await
            .map_err(|error| Error::temporary(format!("provider request failed: {error}")))?;

        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(Error::Format(
                "provider response is larger than the supported limit".to_owned(),
            ));
        }

        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .map_err(|error| {
                Error::temporary(format!("could not read provider response: {error}"))
            })?
            .to_vec();

        if body.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Format(
                "provider response is larger than the supported limit".to_owned(),
            ));
        }

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}
