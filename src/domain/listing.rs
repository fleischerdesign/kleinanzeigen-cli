//! Listing identifiers, prices and listings.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;
use time::OffsetDateTime;
use url::Url;

use crate::error::{Error, Result};

/// A Kleinanzeigen listing identifier.
///
/// The inner value is private and deserialization is deliberately not derived:
/// every identifier goes through [`ListingId::new`], so an invalid value cannot
/// enter the domain through a provider response.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ListingId(String);

impl ListingId {
    /// Creates an identifier from a numeric string.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value: String = value.into();
        if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
            return Err(Error::Input(
                "listing id must be a non-empty numeric string".to_owned(),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ListingId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl fmt::Display for ListingId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A parsed price.
///
/// Fields are private and deserialization is not derived. Providers deserialize
/// their own DTOs and construct a validated value through [`Price::new`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Price {
    amount_cents: i64,
    currency: String,
    negotiable: bool,
}

impl Price {
    /// Creates a price from euro cents and a currency code.
    pub fn new(amount_cents: i64, currency: impl Into<String>, negotiable: bool) -> Result<Self> {
        if amount_cents < 0 {
            return Err(Error::Input("price must not be negative".to_owned()));
        }

        let currency: String = currency.into();
        if currency.len() != 3
            || !currency
                .chars()
                .all(|character| character.is_ascii_uppercase())
        {
            return Err(Error::Input(
                "currency must be a three-letter ISO 4217 code".to_owned(),
            ));
        }

        Ok(Self {
            amount_cents,
            currency,
            negotiable,
        })
    }

    /// Returns the amount in euro cents.
    pub const fn amount_cents(&self) -> i64 {
        self.amount_cents
    }

    /// Returns the ISO 4217 currency code.
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// Returns whether the seller accepts negotiation.
    pub const fn negotiable(&self) -> bool {
        self.negotiable
    }
}

/// A human-readable listing location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    /// City or municipality, if the provider exposes one.
    pub city: Option<String>,
    /// Postal code, if the provider exposes one.
    pub postal_code: Option<String>,
}

impl Location {
    /// Creates a location from optional city and postal-code parts.
    pub const fn new(city: Option<String>, postal_code: Option<String>) -> Self {
        Self { city, postal_code }
    }

    /// Returns whether neither part is present.
    pub const fn is_empty(&self) -> bool {
        self.city.is_none() && self.postal_code.is_none()
    }
}

/// A listing category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Category {
    /// Provider category identifier.
    pub id: String,
    /// Human-readable category name.
    pub name: String,
}

/// One provider attribute of a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attribute {
    /// Stable provider attribute name.
    pub name: String,
    /// Human-readable attribute label.
    pub label: String,
    /// Human-readable value.
    pub value: String,
}

/// A classified listing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    /// Listing identifier.
    pub id: ListingId,
    /// Listing title.
    pub title: String,
    /// Canonical public URL.
    pub url: Url,
    /// Parsed price, if the listing shows one.
    pub price: Option<Price>,
    /// Provider price classification, for example `SPECIFIED_AMOUNT` or
    /// `PLEASE_CONTACT`.
    pub price_type: Option<String>,
    /// Human-readable location, if available.
    pub location: Option<Location>,
    /// Plain-text description, if available.
    pub description: Option<String>,
    /// Publication time reported by the provider.
    #[serde(with = "time::serde::rfc3339::option")]
    pub posted_at: Option<OffsetDateTime>,
    /// Provider listing status, for example `ACTIVE`.
    pub status: Option<String>,
    /// Provider ad type, for example `OFFERED` or `WANTED`.
    pub ad_type: Option<String>,
    /// Provider poster type, for example `PRIVATE` or `COMMERCIAL`.
    pub poster_type: Option<String>,
    /// Category, if available.
    pub category: Option<Category>,
    /// Public numeric seller identifier.
    pub seller_id: Option<String>,
    /// Number of rooms, if the provider exposes a numeric room attribute.
    pub rooms: Option<f64>,
    /// Living area in square metres, if the provider exposes it.
    pub size_m2: Option<f64>,
    /// Public image URLs, largest variant first.
    pub images: Vec<Url>,
    /// Provider attributes with their human-readable labels.
    pub attributes: Vec<Attribute>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_id_accepts_digits() {
        let id = ListingId::new("3399373623").expect("valid id");
        assert_eq!(id.as_str(), "3399373623");
        assert_eq!(id.to_string(), "3399373623");
    }

    #[test]
    fn listing_id_rejects_empty_and_non_numeric_values() {
        assert!(ListingId::new("").is_err());
        assert!(ListingId::new("12a").is_err());
        assert!(ListingId::new(" 12").is_err());
    }

    #[test]
    fn price_rejects_negative_amounts_and_invalid_currencies() {
        assert!(Price::new(-1, "EUR", false).is_err());
        assert!(Price::new(100, "eur", false).is_err());
        assert!(Price::new(100, "EURO", false).is_err());
    }

    #[test]
    fn price_accepts_a_valid_value() {
        let price = Price::new(12_500, "EUR", true).expect("valid price");
        assert_eq!(price.amount_cents(), 12_500);
        assert_eq!(price.currency(), "EUR");
        assert!(price.negotiable());
    }

    #[test]
    fn empty_location_is_reported() {
        assert!(Location::new(None, None).is_empty());
        assert!(!Location::new(Some("Berlin".to_owned()), None).is_empty());
    }
}
