//! Search requests.

use serde::Serialize;

use crate::error::{Error, Result};

/// Provider sort order for search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SearchSort {
    /// Newest listings first.
    DateDescending,
    /// Lowest price first.
    PriceAscending,
    /// Highest price first.
    PriceDescending,
    /// Closest listings first.
    DistanceAscending,
}

impl SearchSort {
    /// Returns the value understood by the mobile API.
    pub const fn as_api_value(self) -> &'static str {
        match self {
            Self::DateDescending => "DATE_DESCENDING",
            Self::PriceAscending => "PRICE_ASCENDING",
            Self::PriceDescending => "PRICE_DESCENDING",
            Self::DistanceAscending => "DISTANCE_ASCENDING",
        }
    }
}

/// A search request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchQuery {
    /// Free-text query.
    pub query: String,
    /// Provider category identifier.
    pub category_id: Option<String>,
    /// Provider location identifier.
    pub location_id: Option<String>,
    /// Search radius in kilometres.
    pub radius_km: Option<u32>,
    /// Minimum price in euro cents.
    pub min_price_cents: Option<i64>,
    /// Maximum price in euro cents.
    pub max_price_cents: Option<i64>,
    /// Provider sort order.
    pub sort: Option<SearchSort>,
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

    /// Validates the request constraints that do not belong to the CLI parser.
    pub fn validate(&self) -> Result<()> {
        if self.limit == 0 || self.limit > 50 {
            return Err(Error::Input("limit must be between 1 and 50".to_owned()));
        }

        for (name, value) in [
            ("minimum price", self.min_price_cents),
            ("maximum price", self.max_price_cents),
        ] {
            if value.is_some_and(|value| value < 0) {
                return Err(Error::Input(format!("{name} must not be negative")));
            }
        }

        if let (Some(minimum), Some(maximum)) = (self.min_price_cents, self.max_price_cents) {
            if minimum > maximum {
                return Err(Error::Input(
                    "minimum price must not exceed maximum price".to_owned(),
                ));
            }
        }

        if let Some(category_id) = &self.category_id {
            validate_numeric_id("category id", category_id)?;
        }
        if let Some(location_id) = &self.location_id {
            validate_numeric_id("location id", location_id)?;
        }
        if let Some(cursor) = &self.cursor {
            cursor_page(cursor)?;
        }

        Ok(())
    }

    /// Returns the one-based page selected by the cursor.
    pub fn page(&self) -> Result<u32> {
        match &self.cursor {
            Some(cursor) => cursor_page(cursor),
            None => Ok(1),
        }
    }
}

fn validate_numeric_id(name: &str, value: &str) -> Result<()> {
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(Error::Input(format!(
            "{name} must be a non-empty numeric string"
        )));
    }
    Ok(())
}

fn cursor_page(cursor: &str) -> Result<u32> {
    let page = cursor
        .parse::<u32>()
        .map_err(|_| Error::Input("cursor must be a positive page number".to_owned()))?;
    if page == 0 {
        return Err(Error::Input(
            "cursor must be a positive page number".to_owned(),
        ));
    }
    Ok(page)
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            category_id: None,
            location_id: None,
            radius_km: None,
            min_price_cents: None,
            max_price_cents: None,
            sort: None,
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
        assert!(query.location_id.is_none());
    }

    #[test]
    fn validation_rejects_invalid_limits_and_ranges() {
        let mut query = SearchQuery::new("test");
        query.limit = 0;
        assert!(query.validate().is_err());

        query.limit = 25;
        query.min_price_cents = Some(100);
        query.max_price_cents = Some(99);
        assert!(query.validate().is_err());
    }

    #[test]
    fn page_defaults_to_one_and_rejects_zero() {
        let mut query = SearchQuery::new("test");
        assert_eq!(query.page().expect("default page"), 1);

        query.cursor = Some("2".to_owned());
        assert_eq!(query.page().expect("page two"), 2);

        query.cursor = Some("0".to_owned());
        assert!(query.page().is_err());
    }

    #[test]
    fn sort_values_are_stable() {
        assert_eq!(SearchSort::DateDescending.as_api_value(), "DATE_DESCENDING");
        assert_eq!(SearchSort::PriceAscending.as_api_value(), "PRICE_ASCENDING");
    }
}
