//! Core domain types shared by the CLI and providers.

/// Listing identifiers, prices and listings.
mod listing;

/// Public seller profiles.
mod profile;

/// Search requests.
mod query;

/// Public reference and metadata values.
mod reference;

/// Public listing counters.
mod stats;

pub use listing::{Attribute, Category, Listing, ListingId, Location, Price};
pub use profile::{SellerBadge, SellerProfile};
pub use query::{AdType, SearchQuery, SearchSort};
pub use reference::{CategoryRef, FilterValue, LocationRef, SearchFilter};
pub use stats::AdStats;
