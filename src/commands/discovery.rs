//! Public discovery commands.

use crate::cli::{ProfileArgs, SellerOtherArgs, SimilarArgs, StatsArgs};
use crate::domain::{AdStats, ListingId, SellerProfile};
use crate::error::Result;
use crate::output::{Format, write_json};
use crate::provider::ListingList;

/// Fetches listings similar to one listing.
pub async fn similar(args: SimilarArgs, format: Format) -> Result<()> {
    let id = ListingId::new(args.id)?;
    let provider = super::provider()?;
    let list = provider.similar(&id).await?;
    output_list(&list, format)
}

/// Fetches other active ads from the same seller.
pub async fn seller_other(args: SellerOtherArgs, format: Format) -> Result<()> {
    let id = ListingId::new(args.id)?;
    let provider = super::provider()?;
    let list = provider.seller_other_ads(&id).await?;
    output_list(&list, format)
}

/// Fetches the public homepage gallery.
pub async fn gallery(format: Format) -> Result<()> {
    let provider = super::provider()?;
    let list = provider.gallery().await?;
    output_list(&list, format)
}

/// Fetches one public seller profile.
pub async fn profile(args: ProfileArgs, format: Format) -> Result<()> {
    let provider = super::provider()?;
    let profile = provider.profile(&args.user_id).await?;
    match format {
        Format::Json => write_json(&profile),
        Format::Human => {
            print_profile(&profile);
            Ok(())
        }
    }
}

/// Fetches public listing counters.
pub async fn stats(args: StatsArgs, format: Format) -> Result<()> {
    let id = ListingId::new(args.id)?;
    let provider = super::provider()?;
    let stats = provider.stats(&id).await?;
    match format {
        Format::Json => write_json(&stats),
        Format::Human => {
            print_stats(&stats);
            Ok(())
        }
    }
}

fn output_list(list: &ListingList, format: Format) -> Result<()> {
    match format {
        Format::Json => write_json(list),
        Format::Human => {
            super::print_listing_list(list);
            Ok(())
        }
    }
}

fn print_profile(profile: &SellerProfile) {
    println!("{}", profile.name.as_deref().unwrap_or("(no public name)"));
    println!("id: {}", profile.id);
    if let Some(since) = &profile.since {
        println!("since: {since}");
    }
    if let Some(poster_type) = &profile.poster_type {
        println!("poster: {poster_type}");
    }
    if let Some(initials) = &profile.initials {
        println!("initials: {initials}");
    }
    if let Some(historical) = profile.historical_ads {
        println!("historical ads: {historical}");
    }
    if let Some(online) = profile.online_ads {
        println!("online ads: {online}");
    }
    if let Some(reply_speed) = &profile.reply_speed {
        println!("reply speed: {reply_speed}");
    }
    println!("secure payment: {}", profile.secure_payment);
    if let Some(account_type) = &profile.seller_account_type {
        println!("account type: {account_type}");
    }
    for badge in &profile.badges {
        println!(
            "badge: {}{}{}",
            badge.name,
            badge
                .level
                .map(|level| format!(" (level {level})"))
                .unwrap_or_default(),
            badge
                .value
                .as_deref()
                .map(|value| format!(": {value}"))
                .unwrap_or_default()
        );
    }
}

fn print_stats(stats: &AdStats) {
    println!("id: {}", stats.id);
    println!("views: {}", stats.views);
    if let Some(watchlist) = stats.watchlist {
        println!("watchlist: {watchlist}");
    }
}
