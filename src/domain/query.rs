//! Search requests.

use serde::Serialize;

/// A search request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchQuery {
    /// Free-text query.
    pub query: String,
    /// Location name or postal code.
    pub location: Option<String>,
    /// Search radius in kilometres.
    pub radius_km: Option<u32>,
    /// Minimum price in euro cents.
    pub min_price_cents: Option<i64>,
    /// Maximum price in euro cents.
    pub max_price_cents: Option<i64>,
    /// Maximum number of results per page.
    pub limit: usize,
    /// Opaque cursor returned by a previous page.
    pub cursor: Option<String>,
}

impl SearchQuery {
    /// Creates a query with the default page size.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            location: None,
            radius_km: None,
            min_price_cents: None,
            max_price_cents: None,
            limit: 25,
            cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_stable() {
        assert_eq!(SearchQuery::default().limit, 25);
    }

    #[test]
    fn new_preserves_the_query_and_defaults() {
        let query = SearchQuery::new("raspberry pi");
        assert_eq!(query.query, "raspberry pi");
        assert_eq!(query.limit, 25);
        assert!(query.location.is_none());
    }
}
