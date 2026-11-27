//! Public reference and metadata commands.

use crate::cli::{CategoriesArgs, LocationArgs, LocationsArgs, SearchMetadataArgs};
use crate::domain::{FlagReason, LocationDetail, LocationRef};
use crate::error::{Error, Result};
use crate::output::{Format, write_json};

/// Lists or filters provider categories.
pub async fn categories(args: CategoriesArgs, format: Format) -> Result<()> {
    let provider = super::provider()?;
    let mut entries = provider.categories().await?;

    if let Some(query) = args
        .query
        .as_deref()
        .filter(|query| !query.trim().is_empty())
    {
        let query = query.to_lowercase();
        entries.retain(|entry| {
            entry.name.to_lowercase().contains(&query) || entry.path.to_lowercase().contains(&query)
        });
    }

    output_categories(&entries, format)
}

/// Resolves provider locations.
pub async fn locations(args: LocationsArgs, format: Format) -> Result<()> {
    if args.query.trim().is_empty() {
        return Err(Error::Input("location query must not be empty".to_owned()));
    }

    let provider = super::provider()?;
    let mut entries = provider.locations(&args.query).await?;
    entries.truncate(args.limit as usize);

    output_locations(&entries, format)
}

/// Fetches category search metadata.
pub async fn search_metadata(args: SearchMetadataArgs, format: Format) -> Result<()> {
    let provider = super::provider()?;
    let filters = provider.search_metadata(&args.category).await?;

    match format {
        Format::Json => write_json(&filters),
        Format::Human => {
            for filter in &filters {
                let label = filter.label.as_deref().unwrap_or(&filter.name);
                let kind = filter.kind.as_deref().unwrap_or("-");
                println!("{}\t{}\t{}", filter.name, label, kind);
                for value in &filter.values {
                    println!(
                        "  {}\t{}",
                        value.value,
                        value.label.as_deref().unwrap_or_default()
                    );
                }
            }
            Ok(())
        }
    }
}

fn output_categories(entries: &[crate::domain::CategoryRef], format: Format) -> Result<()> {
    match format {
        Format::Json => write_json(&entries),
        Format::Human => {
            for entry in entries {
                println!("{}\t{}", entry.id, entry.path);
            }
            Ok(())
        }
    }
}

fn output_locations(entries: &[LocationRef], format: Format) -> Result<()> {
    match format {
        Format::Json => write_json(&entries),
        Format::Human => {
            for entry in entries {
                println!("{}\t{}", entry.id, entry.label);
            }
            Ok(())
        }
    }
}

/// Fetches one public location detail.
pub async fn location(args: LocationArgs, format: Format) -> Result<()> {
    let provider = super::provider()?;
    let location = provider.location(&args.id).await?;
    match format {
        Format::Json => write_json(&location),
        Format::Human => {
            print_location(&location);
            Ok(())
        }
    }
}

/// Lists public reporting reasons.
pub async fn flag_reasons(format: Format) -> Result<()> {
    let provider = super::provider()?;
    let reasons = provider.flag_reasons().await?;
    match format {
        Format::Json => write_json(&reasons),
        Format::Human => {
            for reason in &reasons {
                print_flag_reason(reason, 0);
            }
            Ok(())
        }
    }
}

fn print_location(location: &LocationDetail) {
    println!("{}", location.label);
    println!("id: {}", location.id);
    if let Some(parent_id) = &location.parent_id {
        println!("parent: {parent_id}");
    }
    if let Some(child_count) = location.child_count {
        println!("children: {child_count}");
    }
    if let Some(latitude) = location.latitude {
        println!("latitude: {latitude}");
    }
    if let Some(longitude) = location.longitude {
        println!("longitude: {longitude}");
    }
    if let Some(radius) = location.radius_km {
        println!("radius_km: {radius}");
    }
    for child in &location.children {
        println!("child: {}\t{}", child.id, child.label);
    }
}

fn print_flag_reason(reason: &FlagReason, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{indent}{}\t{}", reason.id, reason.title);
    for child in &reason.subreasons {
        print_flag_reason(child, depth + 1);
    }
}
