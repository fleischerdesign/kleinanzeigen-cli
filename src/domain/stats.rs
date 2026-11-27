//! Public listing counters.

use serde::Serialize;

use super::ListingId;

/// Public counters for one listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdStats {
    /// Listing identifier.
    pub id: ListingId,
    /// View count reported by the provider.
    pub views: u64,
    /// Watchlist count, if the provider returned one.
    pub watchlist: Option<u64>,
}
