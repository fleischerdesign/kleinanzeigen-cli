//! Polling command.

use std::collections::HashSet;

use serde::Serialize;

use crate::cli::{WatchArgs, parse_interval};
use crate::domain::Listing;
use crate::error::{Error, Result};
use crate::output::{Format, SCHEMA_VERSION, write_json_line};
use crate::provider::Provider;
use crate::state::State;

#[derive(Serialize)]
struct WatchEvent<'a> {
    schema_version: u32,
    event: &'a str,
    data: &'a Listing,
}

/// Polls a search and emits new listings. The first poll is a snapshot unless
/// the stored seen set is resumed.
pub async fn watch(args: WatchArgs, format: Format) -> Result<()> {
    let (query, interval) = if let Some(name) = &args.saved {
        if !args.filters.is_default() {
            return Err(Error::Input(
                "search filters cannot be combined with --saved".to_owned(),
            ));
        }
        let state = State::open_default()?;
        let saved = state
            .saved_get(name)?
            .ok_or_else(|| Error::NotFound(format!("saved search {name}")))?;
        let interval = parse_interval(&saved.interval).map_err(Error::Input)?;
        (saved.query, interval)
    } else {
        let query = args.to_query().map_err(Error::Input)?;
        let interval = parse_interval(&args.interval).map_err(Error::Input)?;
        (query, interval)
    };

    let persist = args.uses_state();
    let query_key = serde_json::to_string(&query)
        .map_err(|error| Error::Internal(format!("could not key watch state: {error}")))?;
    let mut seen = if persist {
        let state = State::open_default()?;
        if args.fresh {
            state.clear_seen(&query_key)?;
            HashSet::new()
        } else if args.resume {
            state.seen_ids(&query_key)?
        } else {
            HashSet::new()
        }
    } else {
        HashSet::new()
    };

    let provider = super::provider()?;
    let poll_limit = args.poll_limit();
    let mut polls = 0;

    loop {
        polls += 1;
        let page = provider.search(&query).await?;
        let snapshot = polls == 1 && !args.resume;
        let event = if snapshot { "snapshot" } else { "new" };

        for listing in &page.items {
            if seen.insert(listing.id.clone()) {
                emit(event, listing, format)?;
            }
        }

        if persist || args.saved.is_some() {
            let mut state = State::open_default()?;
            let now = now();
            for listing in &page.items {
                state.record_listing(listing, now)?;
                state.mark_seen(&query_key, &listing.id, now)?;
            }
            if let Some(name) = &args.saved {
                state.saved_mark_run(name, now)?;
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

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
