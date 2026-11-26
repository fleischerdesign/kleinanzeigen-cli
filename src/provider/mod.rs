//! The boundary between the CLI and a concrete Kleinanzeigen access path.

use async_trait::async_trait;
use serde::Serialize;

use crate::domain::{Listing, ListingId, SearchQuery};
use crate::error::Result;

/// One page of search results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchPage {
    /// Listings on this page.
    pub items: Vec<Listing>,
    /// Cursor for the next page, if any.
    pub next_cursor: Option<String>,
}

/// Read access to Kleinanzeigen.
///
/// Providers deserialize their own transport DTOs and convert them into the
/// validated domain types. Account mutations belong to a separate capability,
/// not to this read-only trait.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Searches listings.
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage>;

    /// Fetches one listing by identifier.
    async fn listing(&self, id: &ListingId) -> Result<Listing>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    struct FakeProvider;

    #[async_trait]
    impl Provider for FakeProvider {
        async fn search(&self, _query: &SearchQuery) -> Result<SearchPage> {
            Ok(SearchPage {
                items: Vec::new(),
                next_cursor: None,
            })
        }

        async fn listing(&self, id: &ListingId) -> Result<Listing> {
            Err(Error::NotFound(id.to_string()))
        }
    }

    #[tokio::test]
    async fn provider_is_object_safe() {
        let provider: Box<dyn Provider> = Box::new(FakeProvider);
        let page = provider
            .search(&SearchQuery::new("test"))
            .await
            .expect("empty page");
        assert!(page.items.is_empty());

        let missing = ListingId::new("1").expect("valid id");
        assert!(provider.listing(&missing).await.is_err());
    }
}
