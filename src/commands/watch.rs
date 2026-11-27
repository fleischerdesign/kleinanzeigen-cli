//! Polling command.

use std::collections::HashSet;

use serde::Serialize;

use crate::cli::{WatchArgs, parse_interval};
use crate::domain::{Listing, SearchQuery};
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
        let saved = match State::open_existing_read_only()? {
            Some(state) => state.saved_get(name)?,
            None => None,
        }
        .ok_or_else(|| Error::NotFound(format!("saved search {name}")))?;
        let interval = parse_interval(&saved.interval).map_err(Error::Input)?;
        (saved.query, interval)
    } else {
        let query = args.to_query().map_err(Error::Input)?;
        let interval = parse_interval(&args.interval).map_err(Error::Input)?;
        (query, interval)
    };

    let persist = args.uses_state() || args.saved.is_some();
    let query_key = watch_key(&query)?;
    let mut seen = if args.resume {
        match State::open_existing_read_only()? {
            Some(state) => state.seen_ids(&query_key)?,
            None => {
                return Err(Error::NotFound(
                    "stored watch state; run watch --store first".to_owned(),
                ));
            }
        }
    } else {
        HashSet::new()
    };

    // Resolve the provider before touching local state. A failed --fresh must
    // not discard resume progress.
    let provider = super::provider()?;
    let poll_limit = args.poll_limit();
    let mut polls = 0;
    let mut state = None;

    loop {
        polls += 1;
        let page = provider.search(&query).await?;
        let first_poll = polls == 1;
        let snapshot = first_poll && !args.resume;
        let reset_seen = first_poll && args.fresh;
        let event = if snapshot { "snapshot" } else { "new" };

        if reset_seen {
            seen.clear();
        }
        for listing in &page.items {
            if seen.insert(listing.id.clone()) {
                emit(event, listing, format)?;
            }
        }

        if persist {
            if state.is_none() {
                state = Some(State::open_default()?);
            }
            let active = state.as_mut().expect("state was just opened");
            active.record_page(
                &query_key,
                &page.items,
                now(),
                reset_seen,
                args.saved.as_deref(),
            )?;
        }

        if poll_limit > 0 && polls >= poll_limit {
            break;
        }
        tokio::time::sleep(interval).await;
    }

    Ok(())
}

fn watch_key(query: &SearchQuery) -> Result<String> {
    let mut canonical = query.clone();
    canonical.query = canonical
        .query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    canonical.category = trim_optional(canonical.category);
    canonical.location = trim_optional(canonical.location);
    canonical.location_id = trim_optional(canonical.location_id);
    canonical.cursor = trim_optional(canonical.cursor);
    canonical.exclude = canonical
        .exclude
        .iter()
        .map(|term| term.trim().to_owned())
        .filter(|term| !term.is_empty())
        .collect();

    let json = serde_json::to_string(&canonical)
        .map_err(|error| Error::Internal(format!("could not key watch state: {error}")))?;
    Ok(format!("v1:{json}"))
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_key_normalizes_whitespace_and_references() {
        let mut left = SearchQuery::new("  fendt   bianco ");
        left.category = Some("  Fahrräder  ".to_owned());
        left.exclude = vec!["  defekt ".to_owned()];
        let mut right = SearchQuery::new("fendt bianco");
        right.category = Some("Fahrräder".to_owned());
        right.exclude = vec!["defekt".to_owned()];

        assert_eq!(
            watch_key(&left).expect("left key"),
            watch_key(&right).expect("right key")
        );
        assert!(watch_key(&left).expect("key").starts_with("v1:"));
    }

    #[test]
    fn watch_key_changes_with_the_query() {
        let base = SearchQuery::new("fendt bianco");
        let mut changed = base.clone();
        changed.limit = 10;
        assert_ne!(
            watch_key(&base).expect("base"),
            watch_key(&changed).expect("changed")
        );
    }

    #[test]
    fn watch_key_has_a_stable_shape() {
        let key = watch_key(&SearchQuery::new("x")).expect("key");
        assert_eq!(key, WATCH_KEY_GOLDEN);
    }

    const WATCH_KEY_GOLDEN: &str = "v1:{\"query\":\"x\",\"category\":null,\"location\":null,\"location_id\":null,\"radius_km\":null,\"min_price_cents\":null,\"max_price_cents\":null,\"ad_type\":\"OFFERED\",\"poster_type\":null,\"picture_required\":false,\"buy_now_only\":false,\"include_top_ads\":false,\"sort\":null,\"limit\":25,\"pages\":1,\"cursor\":null,\"exclude\":[],\"min_rooms\":null,\"max_rooms\":null,\"min_size_m2\":null,\"max_size_m2\":null}";
}
