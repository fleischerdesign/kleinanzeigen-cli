//! Public seller profiles.

use serde::Serialize;

/// A public seller profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SellerProfile {
    /// Numeric provider account identifier.
    pub id: String,
    /// Public contact name, if the provider exposes one.
    pub name: Option<String>,
    /// Member-since timestamp reported by the provider.
    pub since: Option<String>,
    /// Provider poster type, for example `PRIVATE`.
    pub poster_type: Option<String>,
    /// Public initials.
    pub initials: Option<String>,
    /// Number of historical ads.
    pub historical_ads: Option<u64>,
    /// Number of online ads.
    pub online_ads: Option<u64>,
    /// Public reply-speed indicator.
    pub reply_speed: Option<String>,
    /// Public seller badges.
    pub badges: Vec<SellerBadge>,
    /// Whether the provider advertises secure payment.
    pub secure_payment: bool,
    /// Provider seller account type.
    pub seller_account_type: Option<String>,
}

/// One public seller badge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SellerBadge {
    /// Badge name.
    pub name: String,
    /// Badge level, if any.
    pub level: Option<u64>,
    /// Badge value, if any.
    pub value: Option<String>,
}
