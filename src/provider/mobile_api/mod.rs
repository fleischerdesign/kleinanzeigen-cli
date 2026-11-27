//! Mobile API provider.
//!
//! The mobile API is private and can change with app updates. This module keeps
//! the transport DTOs separate from the domain and fails loudly when the
//! response shape no longer matches.

mod config;
mod convert;
mod dto;
mod html;
mod reference;
mod transport;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
const CATEGORIES_PATH: &str = "api/categories.json";
const LOCATIONS_PATH: &str = "api/locations.json";
const ACCEPT_LANGUAGE_VALUE: &str = "de-DE";
const PAGE_DELAY: Duration = Duration::from_millis(500);

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
        let start_page = query.page()?;
        let size = u32::try_from(query.limit)
            .map_err(|_| Error::Input("limit is too large".to_owned()))?;
        let category_id = self.resolve_category(query.category.as_deref()).await?;
        let location_id = match &query.location_id {
            Some(location_id) => Some(location_id.clone()),
            None => self.resolve_location(query.location.as_deref()).await?,
        };

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut total = 0;
        let mut next_cursor = None;
        let mut pages_fetched = 0;

        for offset in 0..query.pages {
            let page = start_page
                .checked_add(offset)
                .ok_or_else(|| Error::Input("page range is too large".to_owned()))?;
            let page_result = self
                .search_once(
                    query,
                    page,
                    size,
                    category_id.as_deref(),
                    location_id.as_deref(),
                )
                .await?;
            let SearchPage {
                items: page_items,
                total: page_total,
                next_cursor: page_next,
                ..
            } = page_result;

            if pages_fetched == 0 {
                total = page_total;
            }
            pages_fetched += 1;
            let empty = page_items.is_empty();
            for item in page_items {
                if seen.insert(item.id.clone()) {
                    items.push(item);
                }
            }
            if empty {
                next_cursor = None;
                break;
            }
            next_cursor = page_next;
            if next_cursor.is_none() {
                break;
            }
            if offset + 1 < query.pages {
                tokio::time::sleep(PAGE_DELAY).await;
            }
        }

        Ok(SearchPage {
            items,
            total,
            page: start_page,
            size,
            pages_fetched,
            next_cursor,
        })
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

impl MobileApiProvider {
    async fn search_once(
        &self,
        query: &SearchQuery,
        page: u32,
        size: u32,
        category_id: Option<&str>,
        location_id: Option<&str>,
    ) -> Result<SearchPage> {
        let mut url = self.endpoint(SEARCH_PATH)?;

        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("size", &size.to_string());
            pairs.append_pair("adType", query.ad_type.as_api_value());

            if !query.query.trim().is_empty() {
                pairs.append_pair("q", query.query.trim());
            }
            if let Some(category_id) = category_id {
                pairs.append_pair("categoryId", category_id);
            }
            if let Some(location_id) = location_id {
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
            if query.picture_required {
                pairs.append_pair("pictureRequired", "true");
            }
            if query.buy_now_only {
                pairs.append_pair("buyNowOnly", "true");
            }
            if query.include_top_ads {
                pairs.append_pair("includeTopAds", "true");
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

    async fn resolve_category(&self, value: Option<&str>) -> Result<Option<String>> {
        let Some(value) = value else {
            return Ok(None);
        };
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        if value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value.to_owned()));
        }

        let url = self.endpoint(CATEGORIES_PATH)?;
        let body = self.get(url, "category lookup").await?;
        let response: dto::CategoriesResponse = serde_json::from_slice(&body).map_err(|error| {
            Error::Format(format!("could not parse category response: {error}"))
        })?;
        let categories = reference::categories(response.categories.value);
        reference::resolve_category(&categories, value).map(Some)
    }

    async fn resolve_location(&self, value: Option<&str>) -> Result<Option<String>> {
        let Some(value) = value else {
            return Ok(None);
        };
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }

        let mut url = self.endpoint(LOCATIONS_PATH)?;
        url.query_pairs_mut().append_pair("q", value);
        let body = self.get(url, "location lookup").await?;
        let response: dto::LocationsResponse = serde_json::from_slice(&body).map_err(|error| {
            Error::Format(format!("could not parse location response: {error}"))
        })?;
        let locations = reference::locations(response.locations.value);
        reference::resolve_location(&locations, value).map(Some)
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
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use reqwest::StatusCode;

    use super::*;
    use crate::domain::{AdType, SearchQuery, SearchSort};

    const SEARCH_FIXTURE: &str = include_str!("../../../tests/fixtures/search.json");
    const DETAIL_FIXTURE: &str = include_str!("../../../tests/fixtures/detail.json");
    const CATEGORY_FIXTURE: &str = r#"{
        "{http://www.ebayclassifiedsgroup.com/schema/category/v1}categories": {
            "value": {
                "category": [
                    {
                        "id-name": { "value": "Alle Kategorien" },
                        "localized-name": { "value": "Alle Kategorien" },
                        "category": [
                            {
                                "id": "100",
                                "id-name": { "value": "Synthetic parent" },
                                "localized-name": { "value": "Synthetic parent" }
                            }
                        ]
                    }
                ]
            }
        }
    }"#;
    const LOCATION_FIXTURE: &str = r#"{
        "{http://www.ebayclassifiedsgroup.com/schema/location/v1}locations": {
            "value": {
                "location": [
                    {
                        "id": "3331",
                        "id-name": { "value": "Teststadt" },
                        "localized-name": { "value": "Teststadt" }
                    }
                ]
            }
        }
    }"#;

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

    struct SequenceTransport {
        requests: Mutex<Vec<HttpRequest>>,
        responses: Mutex<VecDeque<HttpResponse>>,
    }

    impl SequenceTransport {
        fn new(bodies: &[&str]) -> Arc<Self> {
            let responses = bodies
                .iter()
                .map(|body| HttpResponse {
                    status: StatusCode::OK,
                    headers: HeaderMap::new(),
                    body: body.as_bytes().to_vec(),
                })
                .collect();
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            })
        }

        fn requests(&self) -> std::sync::MutexGuard<'_, Vec<HttpRequest>> {
            self.requests.lock().expect("request lock")
        }
    }

    #[async_trait]
    impl Transport for SequenceTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
            self.requests.lock().expect("request lock").push(request);
            self.responses
                .lock()
                .expect("response lock")
                .pop_front()
                .ok_or_else(|| Error::Internal("missing test response".to_owned()))
        }
    }

    struct RoutingTransport {
        requests: Mutex<Vec<HttpRequest>>,
        search_body: Vec<u8>,
        category_body: Vec<u8>,
        location_body: Vec<u8>,
    }

    impl RoutingTransport {
        fn new(search: &str, categories: &str, locations: &str) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                search_body: search.as_bytes().to_vec(),
                category_body: categories.as_bytes().to_vec(),
                location_body: locations.as_bytes().to_vec(),
            })
        }

        fn requests(&self) -> std::sync::MutexGuard<'_, Vec<HttpRequest>> {
            self.requests.lock().expect("request lock")
        }
    }

    #[async_trait]
    impl Transport for RoutingTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse> {
            let body = match request.url.path() {
                "/api/categories.json" => self.category_body.clone(),
                "/api/locations.json" => self.location_body.clone(),
                _ => self.search_body.clone(),
            };
            self.requests.lock().expect("request lock").push(request);
            Ok(HttpResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body,
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
        query.category = Some("217".to_owned());
        query.location_id = Some("4910".to_owned());
        query.radius_km = Some(10);
        query.min_price_cents = Some(12_50);
        query.max_price_cents = Some(80_00);
        query.ad_type = AdType::Wanted;
        query.picture_required = true;
        query.buy_now_only = true;
        query.include_top_ads = true;
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
        assert_eq!(page.pages_fetched, 1);
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
        assert_eq!(pairs.get("adType").map(String::as_str), Some("WANTED"));
        assert_eq!(pairs.get("categoryId").map(String::as_str), Some("217"));
        assert_eq!(pairs.get("locationId").map(String::as_str), Some("4910"));
        assert_eq!(pairs.get("distance").map(String::as_str), Some("10"));
        assert_eq!(pairs.get("minPrice").map(String::as_str), Some("12.50"));
        assert_eq!(pairs.get("maxPrice").map(String::as_str), Some("80"));
        assert_eq!(
            pairs.get("pictureRequired").map(String::as_str),
            Some("true")
        );
        assert_eq!(pairs.get("buyNowOnly").map(String::as_str), Some("true"));
        assert_eq!(pairs.get("includeTopAds").map(String::as_str), Some("true"));
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

    fn query_map(request: &HttpRequest) -> std::collections::HashMap<String, String> {
        request
            .url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    #[tokio::test]
    async fn multiple_pages_are_merged_and_deduplicated() {
        let transport = SequenceTransport::new(&[SEARCH_FIXTURE, SEARCH_FIXTURE]);
        let provider =
            MobileApiProvider::with_transport(settings(), transport.clone()).expect("provider");
        let mut query = SearchQuery::new("synthetic");
        query.pages = 2;

        let page = provider.search(&query).await.expect("search page");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, 2);
        assert_eq!(page.page, 1);
        assert_eq!(page.pages_fetched, 2);
        assert_eq!(page.next_cursor.as_deref(), Some("3"));

        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        let pages = requests
            .iter()
            .map(|request| query_map(request).get("page").cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(pages, ["1", "2"]);
    }

    #[tokio::test]
    async fn resolves_category_and_location_names_before_searching() {
        let transport = RoutingTransport::new(SEARCH_FIXTURE, CATEGORY_FIXTURE, LOCATION_FIXTURE);
        let provider =
            MobileApiProvider::with_transport(settings(), transport.clone()).expect("provider");
        let mut query = SearchQuery::new("synthetic");
        query.category = Some("Synthetic parent".to_owned());
        query.location = Some("Teststadt".to_owned());

        let page = provider.search(&query).await.expect("search page");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.pages_fetched, 1);

        let requests = transport.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].url.path(), "/api/categories.json");
        assert_eq!(requests[1].url.path(), "/api/locations.json");
        assert_eq!(requests[2].url.path(), "/api/ads.json");
        let pairs = query_map(&requests[2]);
        assert_eq!(pairs.get("categoryId").map(String::as_str), Some("100"));
        assert_eq!(pairs.get("locationId").map(String::as_str), Some("3331"));
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
