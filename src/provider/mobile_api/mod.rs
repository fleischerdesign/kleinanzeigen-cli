//! Mobile API provider.
//!
//! The mobile API is private and can change with app updates. This module keeps
//! the transport DTOs separate from the domain and fails loudly when the
//! response shape no longer matches.

mod config;
mod convert;
mod dto;
mod html;
mod transport;

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::Method;
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, USER_AGENT,
};
use url::Url;
use uuid::Uuid;

use crate::domain::{Listing, ListingId, SearchQuery};
use crate::error::{Error, Result};
use crate::provider::{Provider, SearchPage};

pub use config::{AppApiSettings, BasicAuth, DEFAULT_APP_VERSION, DEFAULT_BASE_URL};

use transport::{HttpRequest, HttpResponse, ReqwestTransport, Transport};

const SEARCH_PATH: &str = "api/ads.json";
const DETAIL_PATH: &str = "api/ads/{id}.json";
const OFFERED_AD_TYPE: &str = "OFFERED";
const ACCEPT_LANGUAGE_VALUE: &str = "de-DE";

/// A read-only provider for the Kleinanzeigen mobile API.
pub struct MobileApiProvider {
    settings: AppApiSettings,
    transport: Arc<dyn Transport>,
    install_id: String,
}

impl MobileApiProvider {
    /// Builds a provider from settings that include the app credential.
    pub fn new(settings: AppApiSettings) -> Result<Self> {
        settings.require_auth()?;
        let transport = ReqwestTransport::new()?;
        Self::with_transport(settings, Arc::new(transport))
    }

    fn with_transport(settings: AppApiSettings, transport: Arc<dyn Transport>) -> Result<Self> {
        settings.require_auth()?;
        Ok(Self {
            settings,
            transport,
            install_id: install_id(),
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.settings
            .base_url()
            .join(path)
            .map_err(|error| Error::Internal(format!("could not build provider URL: {error}")))
    }

    fn headers(&self) -> Result<HeaderMap> {
        let auth = self.settings.require_auth()?;
        let version = self.settings.app_version();
        let mut headers = HeaderMap::new();

        headers.insert(AUTHORIZATION, header_value(auth.authorization_header())?);
        headers.insert("x-ebayk-app", header_value(self.install_id.clone())?);
        headers.insert(
            HeaderName::from_static("x-ecg-user-agent"),
            header_value(format!("ebayk-android-app-{version}"))?,
        );
        headers.insert(
            HeaderName::from_static("x-ecg-user-version"),
            header_value(version.to_owned())?,
        );
        headers.insert(
            USER_AGENT,
            header_value(format!("Kleinanzeigen/{version} (Android 13; Pixel 7)"))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static(ACCEPT_LANGUAGE_VALUE),
        );

        Ok(headers)
    }

    async fn get(&self, url: Url, operation: &str) -> Result<Vec<u8>> {
        let request = HttpRequest {
            method: Method::GET,
            url,
            headers: self.headers()?,
        };
        let started = Instant::now();
        let response = self.transport.send(request).await?;
        tracing::debug!(
            operation,
            status = response.status.as_u16(),
            elapsed_ms = started.elapsed().as_millis(),
            "mobile API request"
        );
        handle_response(response, operation)
    }
}

#[async_trait]
impl Provider for MobileApiProvider {
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage> {
        query.validate()?;
        let page = query.page()?;
        let size = u32::try_from(query.limit)
            .map_err(|_| Error::Input("limit is too large".to_owned()))?;
        let mut url = self.endpoint(SEARCH_PATH)?;

        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("size", &size.to_string());
            pairs.append_pair("adType", OFFERED_AD_TYPE);

            if !query.query.trim().is_empty() {
                pairs.append_pair("q", query.query.trim());
            }
            if let Some(category_id) = &query.category_id {
                pairs.append_pair("categoryId", category_id);
            }
            if let Some(location_id) = &query.location_id {
                pairs.append_pair("locationId", location_id);
            }
            if let Some(radius_km) = query.radius_km {
                pairs.append_pair("distance", &radius_km.to_string());
            }
            if let Some(min_price) = query.min_price_cents {
                pairs.append_pair("minPrice", &euros(min_price));
            }
            if let Some(max_price) = query.max_price_cents {
                pairs.append_pair("maxPrice", &euros(max_price));
            }
            if let Some(sort) = query.sort {
                pairs.append_pair("sortType", sort.as_api_value());
            }
        }

        let body = self.get(url, "search").await?;
        let response: dto::SearchResponse = serde_json::from_slice(&body)
            .map_err(|error| Error::Format(format!("could not parse search response: {error}")))?;
        convert::search_page(response, page, size)
    }

    async fn listing(&self, id: &ListingId) -> Result<Listing> {
        let path = DETAIL_PATH.replace("{id}", id.as_str());
        let url = self.endpoint(&path)?;
        let body = self.get(url, &format!("listing {id}")).await?;
        let response: dto::DetailResponse = serde_json::from_slice(&body)
            .map_err(|error| Error::Format(format!("could not parse listing response: {error}")))?;
        convert::detail(response)
    }
}

fn install_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("{}{millis}", Uuid::new_v4())
}

fn header_value(value: String) -> Result<HeaderValue> {
    HeaderValue::from_str(&value)
        .map_err(|_| Error::Internal("could not build a provider request header".to_owned()))
}

fn euros(cents: i64) -> String {
    if cents % 100 == 0 {
        (cents / 100).to_string()
    } else {
        format!("{}.{:02}", cents / 100, cents % 100)
    }
}

fn handle_response(response: HttpResponse, operation: &str) -> Result<Vec<u8>> {
    let status = response.status;
    let retry_after = retry_after_seconds(&response.headers);

    match status.as_u16() {
        200 => Ok(response.body),
        401 | 403 => Err(Error::Auth(
            "the provider rejected the app credentials; update KLEINANZEIGEN_BASIC_USER and \
             KLEINANZEIGEN_BASIC_PW"
                .to_owned(),
        )),
        404 => Err(Error::NotFound(operation.to_owned())),
        429 => Err(Error::temporary_after(
            "provider rate limit reached (HTTP 429)",
            retry_after,
        )),
        500..=599 => Err(Error::temporary_after(
            format!("provider returned HTTP {}", status.as_u16()),
            retry_after,
        )),
        300..=399 => Err(Error::Format(format!(
            "provider returned an unexpected redirect (HTTP {})",
            status.as_u16()
        ))),
        400..=499 => Err(Error::Input(format!(
            "provider rejected the request (HTTP {})",
            status.as_u16()
        ))),
        _ => Err(Error::Format(format!(
            "provider returned an unexpected HTTP status {}",
            status.as_u16()
        ))),
    }
}

fn retry_after_seconds(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use reqwest::StatusCode;

    use super::*;
    use crate::domain::{SearchQuery, SearchSort};

    const SEARCH_FIXTURE: &str = include_str!("../../../tests/fixtures/search.json");
    const DETAIL_FIXTURE: &str = include_str!("../../../tests/fixtures/detail.json");

    struct FakeTransport {
        requests: Mutex<Vec<HttpRequest>>,
        status: StatusCode,
        headers: HeaderMap,
        body: Vec<u8>,
    }

    impl FakeTransport {
        fn new(status: StatusCode, body: &str) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                status,
                headers: HeaderMap::new(),
                body: body.as_bytes().to_vec(),
            })
        }

        fn with_retry_after(status: StatusCode, retry_after: &str) -> Arc<Self> {
            let mut headers = HeaderMap::new();
            headers.insert(
                reqwest::header::RETRY_AFTER,
                HeaderValue::from_str(retry_after).expect("valid header"),
            );
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                status,
                headers,
                body: Vec::new(),
            })
        }

        fn requests(&self) -> std::sync::MutexGuard<'_, Vec<HttpRequest>> {
            self.requests.lock().expect("request lock")
        }
    }

    #[async_trait]
    impl Transport for FakeTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
            self.requests.lock().expect("request lock").push(request);
            Ok(HttpResponse {
                status: self.status,
                headers: self.headers.clone(),
                body: self.body.clone(),
            })
        }
    }

    fn settings() -> AppApiSettings {
        AppApiSettings::new(
            "https://api.example.invalid",
            "1.2.3",
            Some(BasicAuth::new("test", "secret").expect("valid auth")),
        )
        .expect("valid settings")
    }

    fn query() -> SearchQuery {
        let mut query = SearchQuery::new("raspberry pi");
        query.min_price_cents = Some(12_50);
        query.max_price_cents = Some(80_00);
        query.sort = Some(SearchSort::PriceAscending);
        query.cursor = Some("2".to_owned());
        query
    }

    #[tokio::test]
    async fn search_builds_the_request_and_parses_the_page() {
        let transport = FakeTransport::new(StatusCode::OK, SEARCH_FIXTURE);
        let provider =
            MobileApiProvider::with_transport(settings(), transport.clone()).expect("provider");

        let page = provider.search(&query()).await.expect("search page");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, 2);
        assert_eq!(page.page, 2);
        assert_eq!(page.size, 25);
        assert_eq!(page.next_cursor.as_deref(), Some("3"));

        let first = &page.items[0];
        assert_eq!(first.title, "Synthetic bike & trailer");
        assert_eq!(
            first.description.as_deref(),
            Some("A synthetic description.\nSecond line.")
        );
        assert_eq!(
            first.price.as_ref().map(|price| price.amount_cents()),
            Some(1_250)
        );
        assert!(first.price.as_ref().expect("price").negotiable());
        assert_eq!(
            first
                .location
                .as_ref()
                .and_then(|location| location.city.as_deref()),
            Some("Teststadt")
        );
        assert_eq!(
            first.images[0].as_str(),
            "https://img.example.invalid/synthetic-bike-large.jpg"
        );
        assert_eq!(first.attributes[0].value, "Good");
        assert_eq!(first.price_type.as_deref(), Some("PLEASE_CONTACT"));
        let serialized = serde_json::to_value(first).expect("serialize listing");
        assert!(serialized["posted_at"].is_string());

        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.method, Method::GET);
        assert_eq!(request.url.path(), "/api/ads.json");
        let pairs = request
            .url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs.get("q").map(String::as_str), Some("raspberry pi"));
        assert_eq!(pairs.get("page").map(String::as_str), Some("2"));
        assert_eq!(pairs.get("size").map(String::as_str), Some("25"));
        assert_eq!(pairs.get("adType").map(String::as_str), Some("OFFERED"));
        assert_eq!(pairs.get("minPrice").map(String::as_str), Some("12.50"));
        assert_eq!(pairs.get("maxPrice").map(String::as_str), Some("80"));
        assert_eq!(
            pairs.get("sortType").map(String::as_str),
            Some("PRICE_ASCENDING")
        );

        assert_eq!(
            request
                .headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Basic dGVzdDpzZWNyZXQ=")
        );
        assert_eq!(
            request
                .headers
                .get(HeaderName::from_static("x-ecg-user-version"))
                .and_then(|value| value.to_str().ok()),
            Some("1.2.3")
        );
        assert!(
            request
                .headers
                .get(HeaderName::from_static("x-ebayk-app"))
                .is_some()
        );
    }

    #[tokio::test]
    async fn listing_builds_the_detail_url() {
        let transport = FakeTransport::new(StatusCode::OK, DETAIL_FIXTURE);
        let provider =
            MobileApiProvider::with_transport(settings(), transport.clone()).expect("provider");
        let id = ListingId::new("1234567890").expect("valid id");

        let listing = provider.listing(&id).await.expect("listing");
        assert_eq!(listing.id.as_str(), "1234567890");
        assert_eq!(listing.title, "Synthetic bike & trailer");
        assert_eq!(listing.poster_type.as_deref(), Some("PRIVATE"));
        assert_eq!(
            listing
                .category
                .as_ref()
                .map(|category| category.name.as_str()),
            Some("Synthetic category")
        );
        assert!(listing.posted_at.is_some());

        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/api/ads/1234567890.json");
    }

    #[tokio::test]
    async fn rate_limit_preserves_the_retry_hint() {
        let transport = FakeTransport::with_retry_after(StatusCode::TOO_MANY_REQUESTS, "30");
        let provider = MobileApiProvider::with_transport(settings(), transport).expect("provider");
        let id = ListingId::new("1234567890").expect("valid id");

        let error = provider.listing(&id).await.expect_err("rate limited");
        assert_eq!(error.code(), "temporary");
        assert_eq!(error.retry_after_seconds(), Some(30));
    }

    #[tokio::test]
    async fn missing_listing_is_not_found() {
        let transport = FakeTransport::new(StatusCode::NOT_FOUND, "");
        let provider = MobileApiProvider::with_transport(settings(), transport).expect("provider");
        let id = ListingId::new("1234567890").expect("valid id");

        let error = provider.listing(&id).await.expect_err("not found");
        assert_eq!(error.code(), "not_found");
    }

    #[tokio::test]
    async fn malformed_json_is_a_format_error() {
        let transport = FakeTransport::new(StatusCode::OK, "{}");
        let provider = MobileApiProvider::with_transport(settings(), transport).expect("provider");
        let id = ListingId::new("1234567890").expect("valid id");

        let error = provider.listing(&id).await.expect_err("format error");
        assert_eq!(error.code(), "format");
    }

    #[test]
    fn provider_without_auth_must_not_start() {
        let settings = AppApiSettings::new("https://api.example.invalid", "1.2.3", None)
            .expect("valid settings");
        assert!(MobileApiProvider::new(settings).is_err());
    }

    #[test]
    fn euros_are_formatted_without_floating_point() {
        assert_eq!(euros(8_000), "80");
        assert_eq!(euros(1_250), "12.50");
        assert_eq!(euros(5), "0.05");
    }
}
