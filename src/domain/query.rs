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

/// The kind of ad to search for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdType {
    /// Offered ads, the normal marketplace search.
    #[default]
    Offered,
    /// Wanted ads.
    Wanted,
}

impl AdType {
    /// Returns the value understood by the mobile API.
    pub const fn as_api_value(self) -> &'static str {
        match self {
            Self::Offered => "OFFERED",
            Self::Wanted => "WANTED",
        }
    }
}

/// The public poster type filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PosterType {
    /// Private sellers.
    Private,
    /// Commercial sellers.
    Commercial,
}

impl PosterType {
    /// Returns the value understood by the mobile API.
    pub const fn as_api_value(self) -> &'static str {
        match self {
            Self::Private => "PRIVATE",
            Self::Commercial => "COMMERCIAL",
        }
    }
}

/// A search request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchQuery {
    /// Free-text query.
    pub query: String,
    /// Provider category id or name.
    pub category: Option<String>,
    /// Provider location name or postal code.
    pub location: Option<String>,
    /// Numeric provider location id.
    pub location_id: Option<String>,
    /// Search radius in kilometres.
    pub radius_km: Option<u32>,
    /// Minimum price in euro cents.
    pub min_price_cents: Option<i64>,
    /// Maximum price in euro cents.
    pub max_price_cents: Option<i64>,
    /// Search for offered or wanted ads.
    pub ad_type: AdType,
    /// Restrict to private or commercial sellers.
    pub poster_type: Option<PosterType>,
    /// Only return ads with at least one picture.
    pub picture_required: bool,
    /// Only return ads with a buy-now option.
    pub buy_now_only: bool,
    /// Include paid top ads.
    pub include_top_ads: bool,
    /// Provider sort order.
    pub sort: Option<SearchSort>,
    /// Maximum number of results per page.
    pub limit: usize,
    /// Number of sequential pages to fetch.
    pub pages: u32,
    /// Opaque cursor returned by a previous page.
    pub cursor: Option<String>,
    /// Client-side terms excluded from title and description.
    pub exclude: Vec<String>,
    /// Minimum number of rooms, if the listing exposes one.
    pub min_rooms: Option<f64>,
    /// Maximum number of rooms, if the listing exposes one.
    pub max_rooms: Option<f64>,
    /// Minimum living area in square metres.
    pub min_size_m2: Option<f64>,
    /// Maximum living area in square metres.
    pub max_size_m2: Option<f64>,
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
        if self.pages == 0 || self.pages > 10 {
            return Err(Error::Input("pages must be between 1 and 10".to_owned()));
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

        for (name, value) in [
            ("minimum rooms", self.min_rooms),
            ("maximum rooms", self.max_rooms),
            ("minimum size", self.min_size_m2),
            ("maximum size", self.max_size_m2),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(Error::Input(format!(
                    "{name} must be a finite non-negative number"
                )));
            }
        }

        if let (Some(minimum), Some(maximum)) = (self.min_rooms, self.max_rooms) {
            if minimum > maximum {
                return Err(Error::Input(
                    "minimum rooms must not exceed maximum rooms".to_owned(),
                ));
            }
        }
        if let (Some(minimum), Some(maximum)) = (self.min_size_m2, self.max_size_m2) {
            if minimum > maximum {
                return Err(Error::Input(
                    "minimum size must not exceed maximum size".to_owned(),
                ));
            }
        }

        if self.location.is_some() && self.location_id.is_some() {
            return Err(Error::Input(
                "pass either a location name or a numeric location id, not both".to_owned(),
            ));
        }

        if let Some(category) = &self.category {
            validate_reference("category", category)?;
        }
        if let Some(location) = &self.location {
            validate_reference("location", location)?;
        }
        if let Some(location_id) = &self.location_id {
            validate_numeric_id("location id", location_id)?;
        }
        if let Some(cursor) = &self.cursor {
            cursor_page(cursor)?;
        }

        let start_page = self.page()?;
        if start_page.checked_add(self.pages - 1).is_none() {
            return Err(Error::Input("page range is too large".to_owned()));
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

fn validate_reference(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Input(format!("{name} must not be empty")));
    }
    Ok(())
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
            category: None,
            location: None,
            location_id: None,
            radius_km: None,
            min_price_cents: None,
            max_price_cents: None,
            ad_type: AdType::Offered,
            poster_type: None,
            picture_required: false,
            buy_now_only: false,
            include_top_ads: false,
            sort: None,
            limit: 25,
            pages: 1,
            cursor: None,
            exclude: Vec::new(),
            min_rooms: None,
            max_rooms: None,
            min_size_m2: None,
            max_size_m2: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_and_page_count_are_stable() {
        let query = SearchQuery::default();
        assert_eq!(query.limit, 25);
        assert_eq!(query.pages, 1);
        assert_eq!(query.ad_type, AdType::Offered);
        assert!(query.poster_type.is_none());
        assert!(!query.picture_required);
        assert!(!query.buy_now_only);
        assert!(!query.include_top_ads);
        assert!(query.exclude.is_empty());
        assert!(query.min_rooms.is_none());
        assert!(query.max_size_m2.is_none());
    }

    #[test]
    fn new_preserves_the_query_and_defaults() {
        let query = SearchQuery::new("raspberry pi");
        assert_eq!(query.query, "raspberry pi");
        assert_eq!(query.limit, 25);
        assert!(query.location.is_none());
        assert!(query.location_id.is_none());
    }

    #[test]
    fn validation_rejects_invalid_limits_ranges_and_page_counts() {
        let mut query = SearchQuery::new("test");
        query.limit = 0;
        assert!(query.validate().is_err());

        query.limit = 25;
        query.pages = 0;
        assert!(query.validate().is_err());
        query.pages = 11;
        assert!(query.validate().is_err());

        query.pages = 1;
        query.min_price_cents = Some(100);
        query.max_price_cents = Some(99);
        assert!(query.validate().is_err());

        query.min_price_cents = None;
        query.max_price_cents = None;
        query.min_rooms = Some(3.0);
        query.max_rooms = Some(2.0);
        assert!(query.validate().is_err());

        query.min_rooms = None;
        query.max_rooms = None;
        query.min_size_m2 = Some(-1.0);
        assert!(query.validate().is_err());
    }

    #[test]
    fn validation_rejects_both_location_forms() {
        let mut query = SearchQuery::new("test");
        query.location = Some("Berlin".to_owned());
        query.location_id = Some("3331".to_owned());
        assert!(query.validate().is_err());
    }

    #[test]
    fn reference_names_are_accepted_and_numeric_location_ids_are_validated() {
        let mut query = SearchQuery::new("test");
        query.category = Some("Fahrräder & Zubehör".to_owned());
        query.location = Some("Berlin".to_owned());
        assert!(query.validate().is_ok());

        query.location = None;
        query.location_id = Some("3331".to_owned());
        assert!(query.validate().is_ok());
        query.location_id = Some("Berlin".to_owned());
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
    fn sort_and_ad_type_values_are_stable() {
        assert_eq!(SearchSort::DateDescending.as_api_value(), "DATE_DESCENDING");
        assert_eq!(SearchSort::PriceAscending.as_api_value(), "PRICE_ASCENDING");
        assert_eq!(AdType::Offered.as_api_value(), "OFFERED");
        assert_eq!(AdType::Wanted.as_api_value(), "WANTED");
        assert_eq!(PosterType::Private.as_api_value(), "PRIVATE");
        assert_eq!(PosterType::Commercial.as_api_value(), "COMMERCIAL");
    }
}
