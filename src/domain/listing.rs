//! Listing identifiers, prices and listings.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};

/// A Kleinanzeigen listing identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    /// Amount in euro cents.
    pub amount_cents: i64,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Whether the seller accepts negotiation.
    pub negotiable: bool,
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
}

/// A classified listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// Listing identifier.
    pub id: ListingId,
    /// Listing title.
    pub title: String,
    /// Canonical public URL.
    pub url: Url,
    /// Parsed price, if the listing shows one.
    pub price: Option<Price>,
    /// Human-readable location, if available.
    pub location: Option<String>,
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
        assert_eq!(price.amount_cents, 12_500);
        assert_eq!(price.currency, "EUR");
        assert!(price.negotiable);
    }
}
