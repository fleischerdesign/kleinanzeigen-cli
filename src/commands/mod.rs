//! Command implementations.

mod discovery;
mod reference;
mod watch;

use crate::cli::{GetArgs, SearchArgs};
use crate::domain::{Listing, ListingId, Price, SearchQuery};
use crate::error::Result;
use crate::output::{Format, write_json};
use crate::provider::mobile_api::{AppApiSettings, MobileApiProvider};
use crate::provider::{ListingList, Provider, SearchPage};

pub use discovery::{gallery, images, profile, seller_other, similar, stats, stats_batch};
pub use reference::{categories, flag_reasons, location, locations, search_metadata};
pub use watch::watch;

/// Searches listings and writes the result.
pub async fn search(args: SearchArgs, format: Format) -> Result<()> {
    let query = args.to_query();
    search_query(&query, format).await
}

async fn search_query(query: &SearchQuery, format: Format) -> Result<()> {
    let provider = provider()?;
    let page = provider.search(query).await?;

    match format {
        Format::Json => write_json(&page),
        Format::Human => {
            print_search_page(&page);
            Ok(())
        }
    }
}

pub(crate) fn print_search_page(page: &SearchPage) {
    let pages = page.pages_fetched.max(1);
    if pages == 1 {
        println!(
            "{} result(s), page {} ({} per page)",
            page.total, page.page, page.size
        );
    } else {
        let last_page = page.page.saturating_add(pages - 1);
        println!(
            "{} result(s), pages {}-{} ({} per page, {} fetched)",
            page.total, page.page, last_page, page.size, pages
        );
    }
    for listing in &page.items {
        println!(
            "{}\t{}\t{}\t{}",
            listing.id,
            human_price(listing),
            human_location(listing),
            listing.title
        );
    }
    if let Some(cursor) = &page.next_cursor {
        println!("next: --cursor {cursor}");
    }
}

/// Fetches one listing and writes it.
pub async fn get(args: GetArgs, format: Format) -> Result<()> {
    let id = ListingId::new(args.id)?;
    let provider = provider()?;
    let listing = provider.listing(&id).await?;

    match format {
        Format::Json => write_json(&listing),
        Format::Human => {
            print_listing(&listing);
            Ok(())
        }
    }
}

pub(crate) fn provider() -> Result<MobileApiProvider> {
    let settings = AppApiSettings::from_env()?;
    MobileApiProvider::new(settings)
}

pub(crate) fn print_listing_list(list: &ListingList) {
    println!("{} result(s)", list.total);
    for listing in &list.items {
        println!(
            "{}\t{}\t{}\t{}",
            listing.id,
            human_price(listing),
            human_location(listing),
            listing.title
        );
    }
}

fn print_listing(listing: &Listing) {
    println!("{}", listing.title);
    println!("id: {}", listing.id);
    println!("url: {}", listing.url);
    println!("price: {}", human_price(listing));
    if let Some(location) = human_location_optional(listing) {
        println!("location: {location}");
    }
    if let Some(posted_at) = listing.posted_at {
        match posted_at.format(&time::format_description::well_known::Rfc3339) {
            Ok(posted_at) => println!("posted: {posted_at}"),
            Err(_) => println!("posted: {posted_at}"),
        }
    }
    if let Some(category) = &listing.category {
        println!("category: {} ({})", category.name, category.id);
    }
    if let Some(status) = &listing.status {
        println!("status: {status}");
    }
    if let Some(poster_type) = &listing.poster_type {
        println!("poster: {poster_type}");
    }
    if let Some(ad_type) = &listing.ad_type {
        println!("type: {ad_type}");
    }
    for attribute in &listing.attributes {
        println!("{}: {}", attribute.label, attribute.value);
    }
    for image in &listing.images {
        println!("image: {image}");
    }
    if let Some(description) = &listing.description {
        println!();
        println!("{description}");
    }
}

pub(crate) fn human_price(listing: &Listing) -> String {
    match &listing.price {
        Some(price) => format_price(price),
        None => listing
            .price_type
            .as_deref()
            .map(human_price_type)
            .unwrap_or("price on request")
            .to_owned(),
    }
}

fn format_price(price: &Price) -> String {
    format!(
        "{}.{:02} {}",
        price.amount_cents() / 100,
        price.amount_cents() % 100,
        price.currency()
    )
}

fn human_price_type(price_type: &str) -> &str {
    match price_type {
        "PLEASE_CONTACT" => "price on request",
        "GIVE_AWAY" => "free",
        "NEGOTIABLE" => "negotiable",
        other => other,
    }
}

pub(crate) fn human_location(listing: &Listing) -> String {
    human_location_optional(listing).unwrap_or_else(|| "-".to_owned())
}

fn human_location_optional(listing: &Listing) -> Option<String> {
    let location = listing.location.as_ref()?;
    let mut parts = Vec::new();
    if let Some(postal_code) = &location.postal_code {
        parts.push(postal_code.clone());
    }
    if let Some(city) = &location.city {
        parts.push(city.clone());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_types_have_human_labels() {
        assert_eq!(human_price_type("PLEASE_CONTACT"), "price on request");
        assert_eq!(human_price_type("GIVE_AWAY"), "free");
        assert_eq!(human_price_type("SPECIFIED_AMOUNT"), "SPECIFIED_AMOUNT");
    }
}
