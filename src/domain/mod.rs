//! Core domain types shared by the CLI and providers.

/// Listing identifiers, prices and listings.
mod listing;

/// Search requests.
mod query;

pub use listing::{Listing, ListingId, Price};
pub use query::SearchQuery;
