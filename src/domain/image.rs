//! Public listing images.

use serde::Serialize;
use url::Url;

use super::ListingId;

/// One public image variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageVariant {
    /// Provider relationship, for example `large` or `XXL`.
    pub rel: String,
    /// Public image URL.
    pub url: Url,
}

/// All public image variants of one listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageSet {
    /// Listing identifier.
    pub id: ListingId,
    /// Image variants in provider order.
    pub variants: Vec<ImageVariant>,
}
