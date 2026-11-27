//! Core domain types shared by the CLI and providers.

/// Listing identifiers, prices and listings.
mod listing;

/// Search requests.
mod query;

pub use listing::{Attribute, Category, Listing, ListingId, Location, Price};
pub use query::{AdType, SearchQuery, SearchSort};
