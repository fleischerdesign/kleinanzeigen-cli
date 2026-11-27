//! Polling command.

use std::collections::HashSet;

use serde::Serialize;

use crate::cli::{WatchArgs, parse_interval};
use crate::domain::Listing;
use crate::error::{Error, Result};
use crate::output::{Format, SCHEMA_VERSION, write_json_line};
use crate::provider::Provider;

#[derive(Serialize)]
struct WatchEvent<'a> {
    schema_version: u32,
    event: &'a str,
    data: &'a Listing,
}

/// Polls a search and emits new listings. The first poll is a snapshot.
pub async fn watch(args: WatchArgs, format: Format) -> Result<()> {
    let interval = parse_interval(&args.interval).map_err(Error::Input)?;
    let query = args.to_query();
    let poll_limit = args.poll_limit();
    let provider = super::provider()?;
    let mut seen = HashSet::new();
    let mut polls = 0;

    loop {
        polls += 1;
        let page = provider.search(&query).await?;
        let event = if polls == 1 { "snapshot" } else { "new" };

        for listing in page.items {
            if seen.insert(listing.id.clone()) {
                emit(event, &listing, format)?;
            }
        }

        if poll_limit > 0 && polls >= poll_limit {
            break;
        }
        tokio::time::sleep(interval).await;
    }

    Ok(())
}

fn emit(event: &str, listing: &Listing, format: Format) -> Result<()> {
    match format {
        Format::Json => write_json_line(&WatchEvent {
            schema_version: SCHEMA_VERSION,
            event,
            data: listing,
        }),
        Format::Human => {
            println!(
                "{event}\t{}\t{}\t{}\t{}",
                listing.id,
                super::human_price(listing),
                super::human_location(listing),
                listing.title
            );
            Ok(())
        }
    }
}
