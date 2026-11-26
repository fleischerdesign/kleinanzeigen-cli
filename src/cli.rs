//! Command-line surface.

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::domain::{SearchQuery, SearchSort};
use crate::output::Format;

/// Unofficial, browser-free CLI for Kleinanzeigen.
#[derive(Debug, Parser)]
#[command(
    name = "kleinanzeigen",
    version,
    about = "Unofficial, browser-free CLI for Kleinanzeigen",
    long_about = None,
    arg_required_else_help = true,
    color = clap::ColorChoice::Never
)]
pub struct Cli {
    /// Emit machine-readable JSON on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Log filter (error, warn, info, debug, trace).
    #[arg(long, global = true, env = "KLEINANZEIGEN_LOG", default_value = "warn")]
    pub log_level: String,

    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Returns the selected output format.
    pub const fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else {
            Format::Human
        }
    }
}

/// Available commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the local installation and provider configuration.
    Doctor,

    /// Search listings.
    Search(SearchArgs),

    /// Fetch one listing by numeric identifier.
    Get(GetArgs),
}

/// Arguments for `search`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Free-text query. Pass an empty string to browse a category.
    pub query: String,

    /// Provider category identifier.
    #[arg(long, value_name = "ID", value_parser = parse_numeric_id)]
    pub category: Option<String>,

    /// Provider location identifier.
    #[arg(long = "location-id", value_name = "ID", value_parser = parse_numeric_id)]
    pub location_id: Option<String>,

    /// Search radius in kilometres.
    #[arg(long, value_name = "KM", value_parser = clap::value_parser!(u32).range(0..=1000))]
    pub radius: Option<u32>,

    /// Minimum price in euros, for example 12.50.
    #[arg(long = "min-price", value_name = "EUROS", value_parser = parse_price_cents)]
    pub min_price_cents: Option<i64>,

    /// Maximum price in euros, for example 80.
    #[arg(long = "max-price", value_name = "EUROS", value_parser = parse_price_cents)]
    pub max_price_cents: Option<i64>,

    /// Page size, 1 to 50.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 25,
        value_parser = clap::value_parser!(u32).range(1..=50)
    )]
    pub limit: u32,

    /// Opaque cursor from a previous page.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,

    /// Sort order.
    #[arg(long, value_name = "ORDER", value_enum)]
    pub sort: Option<SortArg>,
}

impl SearchArgs {
    /// Converts the parsed arguments into a domain query.
    pub fn to_query(&self) -> SearchQuery {
        SearchQuery {
            query: self.query.clone(),
            category_id: self.category.clone(),
            location_id: self.location_id.clone(),
            radius_km: self.radius,
            min_price_cents: self.min_price_cents,
            max_price_cents: self.max_price_cents,
            sort: self.sort.map(SearchSort::from),
            limit: self.limit as usize,
            cursor: self.cursor.clone(),
        }
    }
}

/// Arguments for `get`.
#[derive(Debug, Args)]
pub struct GetArgs {
    /// Numeric listing identifier.
    pub id: String,
}

/// Sort order accepted on the command line.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SortArg {
    /// Newest listings first.
    DateDescending,
    /// Lowest price first.
    PriceAscending,
    /// Highest price first.
    PriceDescending,
    /// Closest listings first.
    DistanceAscending,
}

impl From<SortArg> for SearchSort {
    fn from(value: SortArg) -> Self {
        match value {
            SortArg::DateDescending => Self::DateDescending,
            SortArg::PriceAscending => Self::PriceAscending,
            SortArg::PriceDescending => Self::PriceDescending,
            SortArg::DistanceAscending => Self::DistanceAscending,
        }
    }
}

fn parse_price_cents(value: &str) -> Result<i64, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("price must not be empty".to_owned());
    }
    if value.starts_with('-') {
        return Err("price must not be negative".to_owned());
    }

    let (euros, cents) = value.split_once('.').unwrap_or((value, ""));
    if euros.is_empty() || !euros.chars().all(|character| character.is_ascii_digit()) {
        return Err("price must be a decimal number, for example 12.50".to_owned());
    }
    if cents.len() > 2 || !cents.chars().all(|character| character.is_ascii_digit()) {
        return Err("price must have at most two decimal places".to_owned());
    }

    let euros = euros
        .parse::<i64>()
        .map_err(|_| "price is out of range".to_owned())?;
    let cents = match cents.len() {
        0 => 0,
        1 => i64::from(cents.as_bytes()[0] - b'0') * 10,
        _ => cents
            .parse::<i64>()
            .map_err(|_| "price is out of range".to_owned())?,
    };

    euros
        .checked_mul(100)
        .and_then(|value| value.checked_add(cents))
        .ok_or_else(|| "price is out of range".to_owned())
}

fn parse_numeric_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Err("identifier must be a non-empty numeric string".to_owned());
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_parse_to_cents_without_floating_point() {
        assert_eq!(parse_price_cents("80").expect("price"), 8_000);
        assert_eq!(parse_price_cents("12.5").expect("price"), 1_250);
        assert_eq!(parse_price_cents("12.50").expect("price"), 1_250);
        assert_eq!(parse_price_cents("0.05").expect("price"), 5);
    }

    #[test]
    fn invalid_prices_are_rejected() {
        assert!(parse_price_cents("").is_err());
        assert!(parse_price_cents("-1").is_err());
        assert!(parse_price_cents("12.345").is_err());
        assert!(parse_price_cents("1.2.3").is_err());
        assert!(parse_price_cents("abc").is_err());
    }

    #[test]
    fn search_arguments_convert_to_a_query() {
        let cli = Cli::try_parse_from([
            "kleinanzeigen",
            "search",
            "raspberry pi",
            "--min-price",
            "12.50",
            "--sort",
            "price-ascending",
            "--limit",
            "10",
        ])
        .expect("valid arguments");

        let Command::Search(args) = cli.command else {
            panic!("expected search command");
        };
        let query = args.to_query();
        assert_eq!(query.query, "raspberry pi");
        assert_eq!(query.min_price_cents, Some(1_250));
        assert_eq!(query.sort, Some(SearchSort::PriceAscending));
        assert_eq!(query.limit, 10);
    }

    #[test]
    fn numeric_ids_are_validated() {
        assert_eq!(parse_numeric_id("123").expect("id"), "123");
        assert!(parse_numeric_id("12a").is_err());
        assert!(parse_numeric_id("").is_err());
    }
}
