//! Local state commands: history, saved searches and export.

use serde::Serialize;

use crate::cli::{
    ExportArgs, ExportFormat, ExportKind, HistoryArgs, SavedCommand, SavedNameArgs, parse_interval,
};
use crate::domain::ListingId;
use crate::error::{Error, Result};
use crate::output::{Format, write_json, write_json_line};
use crate::state::{SavedSearch, State};

/// Shows locally recorded price history.
pub fn history(args: HistoryArgs, format: Format) -> Result<()> {
    let id = ListingId::new(args.id)?;
    let state = State::open_default()?;
    let points = state.history(&id)?;

    match format {
        Format::Json => write_json(&points),
        Format::Human => {
            for point in &points {
                println!(
                    "{}\t{}.{:02} {}\t{}",
                    point.seen_at,
                    point.amount_cents / 100,
                    point.amount_cents % 100,
                    point.currency,
                    point.price_type.as_deref().unwrap_or("-")
                );
            }
            Ok(())
        }
    }
}

/// Manages saved searches.
pub fn saved(command: SavedCommand, format: Format) -> Result<()> {
    let state = State::open_default()?;

    match command {
        SavedCommand::Add(args) => {
            let name = args.name.trim();
            if name.is_empty() {
                return Err(Error::Input(
                    "saved search name must not be empty".to_owned(),
                ));
            }
            parse_interval(&args.interval).map_err(Error::Input)?;
            let query = args.to_query();
            query.validate()?;
            state.saved_add(name, &query, &args.interval, now())?;
            let saved = state
                .saved_get(name)?
                .ok_or_else(|| Error::Internal("saved search disappeared".to_owned()))?;
            output_saved(&saved, format)
        }
        SavedCommand::List => {
            let saved = state.saved_list()?;
            match format {
                Format::Json => write_json(&saved),
                Format::Human => {
                    for entry in &saved {
                        println!("{}\t{}\t{}", entry.name, entry.interval, entry.query.query);
                    }
                    Ok(())
                }
            }
        }
        SavedCommand::Show(args) => {
            let name = checked_name(&args);
            let saved = state
                .saved_get(name)?
                .ok_or_else(|| Error::NotFound(format!("saved search {name}")))?;
            output_saved(&saved, format)
        }
        SavedCommand::Remove(args) => {
            let name = checked_name(&args);
            let removed = state.saved_remove(name)?;
            let result = Removed {
                name: name.to_owned(),
                removed,
            };
            match format {
                Format::Json => write_json(&result),
                Format::Human => {
                    println!(
                        "{}: {}",
                        result.name,
                        if result.removed {
                            "removed"
                        } else {
                            "not found"
                        }
                    );
                    Ok(())
                }
            }
        }
    }
}

/// Exports locally recorded state.
pub fn export(args: ExportArgs, _format: Format) -> Result<()> {
    let listing = args.listing.as_deref();
    let state = State::open_default()?;

    match args.kind {
        ExportKind::History => {
            let rows = state.export_history(listing, args.limit)?;
            match args.format {
                ExportFormat::Jsonl => {
                    for row in &rows {
                        write_json_line(row)?;
                    }
                    Ok(())
                }
                ExportFormat::Csv => {
                    println!("listing_id,seen_at,amount_cents,currency,price_type");
                    for row in &rows {
                        println!(
                            "{},{},{},{},{}",
                            csv_field(&row.listing_id),
                            row.seen_at,
                            row.amount_cents,
                            csv_field(&row.currency),
                            csv_field(row.price_type.as_deref().unwrap_or(""))
                        );
                    }
                    Ok(())
                }
            }
        }
        ExportKind::Listings => {
            let rows = state.export_listings(listing, args.limit)?;
            match args.format {
                ExportFormat::Jsonl => {
                    for row in &rows {
                        write_json_line(row)?;
                    }
                    Ok(())
                }
                ExportFormat::Csv => {
                    println!(
                        "id,first_seen,last_seen,title,url,status,category,city,\
                         postal_code,rooms,size_m2,last_price_cents,currency,price_type"
                    );
                    for row in &rows {
                        println!(
                            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                            csv_field(&row.id),
                            row.first_seen,
                            row.last_seen,
                            csv_field(&row.title),
                            csv_field(&row.url),
                            csv_field(row.status.as_deref().unwrap_or("")),
                            csv_field(row.category.as_deref().unwrap_or("")),
                            csv_field(row.city.as_deref().unwrap_or("")),
                            csv_field(row.postal_code.as_deref().unwrap_or("")),
                            optional_number(row.rooms),
                            optional_number(row.size_m2),
                            row.last_price_cents
                                .map(|cents| cents.to_string())
                                .unwrap_or_default(),
                            csv_field(row.currency.as_deref().unwrap_or("")),
                            csv_field(row.price_type.as_deref().unwrap_or(""))
                        );
                    }
                    Ok(())
                }
            }
        }
    }
}

#[derive(Serialize)]
struct Removed {
    name: String,
    removed: bool,
}

fn output_saved(saved: &SavedSearch, format: Format) -> Result<()> {
    match format {
        Format::Json => write_json(saved),
        Format::Human => {
            println!("{}", saved.name);
            println!("interval: {}", saved.interval);
            println!("query: {}", saved.query.query);
            Ok(())
        }
    }
}

fn checked_name(args: &SavedNameArgs) -> &str {
    args.name.trim()
}

fn csv_field(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn optional_number(value: Option<f64>) -> String {
    value.map(|number| number.to_string()).unwrap_or_default()
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
