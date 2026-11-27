//! Command-line surface.

use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::domain::{AdType, PosterType, SearchQuery, SearchSort};
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

    /// List or filter provider categories.
    Categories(CategoriesArgs),

    /// Resolve provider locations.
    Locations(LocationsArgs),

    /// Fetch listings similar to one listing.
    Similar(SimilarArgs),

    /// Fetch other active ads from the same seller.
    SellerOther(SellerOtherArgs),

    /// Fetch the public homepage gallery.
    Gallery(GalleryArgs),

    /// Fetch all public image variants of one listing.
    Images(ImagesArgs),

    /// Fetch a public seller profile.
    Profile(ProfileArgs),

    /// Fetch public view and watchlist counters.
    Stats(StatsArgs),

    /// Fetch public counters for one or more listings.
    StatsBatch(StatsBatchArgs),

    /// Fetch one public location detail.
    Location(LocationArgs),

    /// List public reporting reasons.
    FlagReasons,

    /// Fetch search metadata for a category.
    SearchMetadata(SearchMetadataArgs),

    /// Poll a search and emit new listings as NDJSON.
    Watch(WatchArgs),

    /// Show locally recorded price history.
    History(HistoryArgs),

    /// Manage locally saved searches.
    Saved(SavedArgs),

    /// Export locally recorded price history.
    Export(ExportArgs),
}

/// Arguments shared by search and watch.
#[derive(Debug, Args)]
pub struct SearchFilters {
    /// Provider category id or name.
    #[arg(long, value_name = "ID|NAME", value_parser = parse_reference_value)]
    pub category: Option<String>,

    /// Provider location name or postal code.
    #[arg(
        long,
        value_name = "NAME|POSTCODE",
        value_parser = parse_reference_value,
        conflicts_with = "location_id"
    )]
    pub location: Option<String>,

    /// Numeric provider location id.
    #[arg(
        long = "location-id",
        value_name = "ID",
        value_parser = parse_numeric_id,
        conflicts_with = "location"
    )]
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

    /// Search for offered or wanted ads.
    #[arg(long, value_name = "TYPE", value_enum, default_value = "offered")]
    pub ad_type: AdTypeArg,

    /// Restrict to private or commercial sellers.
    #[arg(long, value_name = "TYPE", value_enum)]
    pub poster_type: Option<PosterTypeArg>,

    /// Only return ads with at least one picture.
    #[arg(long)]
    pub picture_required: bool,

    /// Only return ads with a buy-now option.
    #[arg(long)]
    pub buy_now_only: bool,

    /// Include paid top ads.
    #[arg(long)]
    pub include_top_ads: bool,

    /// Sort order.
    #[arg(long, value_name = "ORDER", value_enum)]
    pub sort: Option<SortArg>,

    /// Page size, 1 to 50.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 25,
        value_parser = clap::value_parser!(u32).range(1..=50)
    )]
    pub limit: u32,

    /// Exclude listings whose title or description contains this term.
    #[arg(long, value_name = "TERM", value_parser = parse_filter_term)]
    pub exclude: Vec<String>,

    /// Minimum number of rooms.
    #[arg(long = "min-rooms", value_name = "N", value_parser = parse_non_negative_f64)]
    pub min_rooms: Option<f64>,

    /// Maximum number of rooms.
    #[arg(long = "max-rooms", value_name = "N", value_parser = parse_non_negative_f64)]
    pub max_rooms: Option<f64>,

    /// Minimum living area in square metres.
    #[arg(long = "min-size", value_name = "M2", value_parser = parse_non_negative_f64)]
    pub min_size_m2: Option<f64>,

    /// Maximum living area in square metres.
    #[arg(long = "max-size", value_name = "M2", value_parser = parse_non_negative_f64)]
    pub max_size_m2: Option<f64>,
}

impl SearchFilters {
    /// Returns whether every filter is at its default value.
    pub fn is_default(&self) -> bool {
        self.category.is_none()
            && self.location.is_none()
            && self.location_id.is_none()
            && self.radius.is_none()
            && self.min_price_cents.is_none()
            && self.max_price_cents.is_none()
            && self.ad_type == AdTypeArg::Offered
            && self.poster_type.is_none()
            && !self.picture_required
            && !self.buy_now_only
            && !self.include_top_ads
            && self.sort.is_none()
            && self.limit == 25
            && self.exclude.is_empty()
            && self.min_rooms.is_none()
            && self.max_rooms.is_none()
            && self.min_size_m2.is_none()
            && self.max_size_m2.is_none()
    }

    /// Applies the shared filters to a search query.
    pub fn apply(&self, query: &mut SearchQuery) {
        query.category = self.category.clone();
        query.location = self.location.clone();
        query.location_id = self.location_id.clone();
        query.radius_km = self.radius;
        query.min_price_cents = self.min_price_cents;
        query.max_price_cents = self.max_price_cents;
        query.ad_type = self.ad_type.into();
        query.poster_type = self.poster_type.map(PosterType::from);
        query.picture_required = self.picture_required;
        query.buy_now_only = self.buy_now_only;
        query.include_top_ads = self.include_top_ads;
        query.sort = self.sort.map(SearchSort::from);
        query.limit = self.limit as usize;
        query.exclude = self.exclude.clone();
        query.min_rooms = self.min_rooms;
        query.max_rooms = self.max_rooms;
        query.min_size_m2 = self.min_size_m2;
        query.max_size_m2 = self.max_size_m2;
    }
}

/// Arguments for `search`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Free-text query. Pass an empty string to browse a category.
    pub query: String,

    /// Shared search filters.
    #[command(flatten)]
    pub filters: SearchFilters,

    /// Number of sequential pages to fetch, 1 to 10.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 1,
        value_parser = clap::value_parser!(u32).range(1..=10)
    )]
    pub pages: u32,

    /// Opaque cursor from a previous page.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
}

impl SearchArgs {
    /// Converts the parsed arguments into a domain query.
    pub fn to_query(&self) -> SearchQuery {
        let mut query = SearchQuery::new(self.query.as_str());
        self.filters.apply(&mut query);
        query.pages = self.pages;
        query.cursor = self.cursor.clone();
        query
    }
}

/// Arguments for `watch`.
#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Free-text query. Omit when using --saved.
    pub query: Option<String>,

    /// Shared search filters.
    #[command(flatten)]
    pub filters: SearchFilters,

    /// Use a saved search instead of a query and filters.
    #[arg(long, value_name = "NAME", conflicts_with = "query")]
    pub saved: Option<String>,

    /// Poll interval: 30s, 15m or 1h.
    #[arg(long, default_value = "15m", value_name = "DURATION")]
    pub interval: String,

    /// Stop after this many polls. The default is unlimited.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=100000))]
    pub max_polls: Option<u32>,

    /// Fetch one snapshot and exit.
    #[arg(long, conflicts_with = "max_polls")]
    pub once: bool,

    /// Persist seen ids and price history in the local state database.
    #[arg(long)]
    pub store: bool,

    /// Resume from the stored seen set and suppress the initial snapshot.
    #[arg(long, conflicts_with = "fresh")]
    pub resume: bool,

    /// Clear the stored seen set and start with a new snapshot.
    #[arg(long, conflicts_with = "resume")]
    pub fresh: bool,
}

impl WatchArgs {
    /// Converts the parsed arguments into a one-page watch query.
    pub fn to_query(&self) -> Result<SearchQuery, String> {
        let query = self
            .query
            .as_deref()
            .ok_or_else(|| "watch needs a query or --saved".to_owned())?;
        let mut query = SearchQuery::new(query);
        self.filters.apply(&mut query);
        query.pages = 1;
        query.cursor = None;
        if query.sort.is_none() {
            query.sort = Some(SearchSort::DateDescending);
        }
        Ok(query)
    }

    /// Returns the maximum number of polls, where zero means unlimited.
    pub fn poll_limit(&self) -> u32 {
        if self.once {
            1
        } else {
            self.max_polls.unwrap_or(0)
        }
    }

    /// Returns whether the local state database is used.
    pub const fn uses_state(&self) -> bool {
        self.store || self.resume || self.fresh
    }
}

/// Arguments for `history`.
#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Numeric listing identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub id: String,
}

/// Arguments for `saved`.
#[derive(Debug, Args)]
pub struct SavedArgs {
    /// Saved-search command.
    #[command(subcommand)]
    pub command: SavedCommand,
}

/// Subcommands for `saved`.
#[derive(Debug, Subcommand)]
pub enum SavedCommand {
    /// Store or replace a saved search.
    Add(Box<SavedAddArgs>),
    /// List saved searches.
    List,
    /// Show one saved search.
    Show(SavedNameArgs),
    /// Remove one saved search.
    Remove(SavedNameArgs),
}

/// Arguments for `saved add`.
#[derive(Debug, Args)]
pub struct SavedAddArgs {
    /// User-chosen name.
    pub name: String,
    /// Free-text query.
    pub query: String,
    /// Shared search filters.
    #[command(flatten)]
    pub filters: SearchFilters,
    /// Watch interval: 30s, 15m or 1h.
    #[arg(long, default_value = "15m", value_name = "DURATION")]
    pub interval: String,
}

impl SavedAddArgs {
    /// Converts the parsed arguments into a one-page saved query.
    pub fn to_query(&self) -> SearchQuery {
        let mut query = SearchQuery::new(self.query.as_str());
        self.filters.apply(&mut query);
        query.pages = 1;
        query.cursor = None;
        if query.sort.is_none() {
            query.sort = Some(SearchSort::DateDescending);
        }
        query
    }
}

/// Arguments that name one saved search.
#[derive(Debug, Args)]
pub struct SavedNameArgs {
    /// Saved search name.
    pub name: String,
}

/// Export format.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    /// One JSON object per line.
    Jsonl,
    /// Comma-separated rows.
    Csv,
}

/// What an export contains.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportKind {
    /// Recorded price points.
    History,
    /// Stored listings with their last observed values.
    Listings,
}

/// Arguments for `export`.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Output format.
    #[arg(long, value_enum, default_value = "jsonl")]
    pub format: ExportFormat,

    /// Dataset to export.
    #[arg(long, value_enum, default_value = "history")]
    pub kind: ExportKind,

    /// Restrict the export to one listing.
    #[arg(long, value_name = "ID", value_parser = parse_numeric_id)]
    pub listing: Option<String>,

    /// Maximum number of rows.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 10_000,
        value_parser = clap::value_parser!(u32).range(1..=100_000)
    )]
    pub limit: u32,
}

/// Arguments for `get`.
#[derive(Debug, Args)]
pub struct GetArgs {
    /// Numeric listing identifier.
    pub id: String,
}

/// Arguments for `categories`.
#[derive(Debug, Args)]
pub struct CategoriesArgs {
    /// Optional case-insensitive name or path filter.
    pub query: Option<String>,
}

/// Arguments for `locations`.
#[derive(Debug, Args)]
pub struct LocationsArgs {
    /// Place name or postal code.
    pub query: String,

    /// Maximum number of matches.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 25,
        value_parser = clap::value_parser!(u32).range(1..=100)
    )]
    pub limit: u32,
}

/// Arguments for `similar`.
#[derive(Debug, Args)]
pub struct SimilarArgs {
    /// Numeric listing identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub id: String,
}

/// Arguments for `seller-other`.
#[derive(Debug, Args)]
pub struct SellerOtherArgs {
    /// Numeric listing identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub id: String,
}

/// Arguments for `gallery`.
#[derive(Debug, Args)]
pub struct GalleryArgs {
    /// Number of gallery pages to fetch, 1 to 10.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 1,
        value_parser = clap::value_parser!(u32).range(1..=10)
    )]
    pub pages: u32,

    /// Page size, 1 to 50.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(1..=50)
    )]
    pub limit: u32,

    /// Opaque cursor from a previous gallery page.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
}

impl GalleryArgs {
    /// Returns the one-based gallery page selected by the cursor.
    pub fn start_page(&self) -> Result<u32, String> {
        match &self.cursor {
            Some(cursor) => parse_page_cursor(cursor),
            None => Ok(1),
        }
    }
}

/// Arguments for `images`.
#[derive(Debug, Args)]
pub struct ImagesArgs {
    /// Numeric listing identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub id: String,
}

/// Arguments for `profile`.
#[derive(Debug, Args)]
pub struct ProfileArgs {
    /// Numeric seller identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub user_id: String,
}

/// Arguments for `stats`.
#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Numeric listing identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub id: String,
}

/// Arguments for `stats-batch`.
#[derive(Debug, Args)]
pub struct StatsBatchArgs {
    /// Numeric listing identifiers.
    #[arg(
        required = true,
        value_name = "ID",
        value_parser = parse_numeric_id,
        num_args = 1..=20
    )]
    pub ids: Vec<String>,
}

/// Arguments for `location`.
#[derive(Debug, Args)]
pub struct LocationArgs {
    /// Numeric location identifier.
    #[arg(value_parser = parse_numeric_id)]
    pub id: String,
}

/// Arguments for `search-metadata`.
#[derive(Debug, Args)]
pub struct SearchMetadataArgs {
    /// Provider category id or name.
    #[arg(value_parser = parse_reference_value)]
    pub category: String,
}

/// Ad type accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AdTypeArg {
    /// Offered ads.
    Offered,
    /// Wanted ads.
    Wanted,
}

impl From<AdTypeArg> for AdType {
    fn from(value: AdTypeArg) -> Self {
        match value {
            AdTypeArg::Offered => Self::Offered,
            AdTypeArg::Wanted => Self::Wanted,
        }
    }
}

/// Poster type accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PosterTypeArg {
    /// Private sellers.
    Private,
    /// Commercial sellers.
    Commercial,
}

impl From<PosterTypeArg> for PosterType {
    fn from(value: PosterTypeArg) -> Self {
        match value {
            PosterTypeArg::Private => Self::Private,
            PosterTypeArg::Commercial => Self::Commercial,
        }
    }
}

/// Sort order accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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

fn parse_reference_value(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("value must not be empty".to_owned());
    }
    Ok(value.to_owned())
}

fn parse_filter_term(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("exclude term must not be empty".to_owned());
    }
    Ok(value.to_owned())
}

fn parse_non_negative_f64(value: &str) -> Result<f64, String> {
    let value = value.trim().replace(',', ".");
    let value = value
        .parse::<f64>()
        .map_err(|_| "value must be a finite non-negative number".to_owned())?;
    if !value.is_finite() || value < 0.0 {
        return Err("value must be a finite non-negative number".to_owned());
    }
    Ok(value)
}

/// Parses a watch interval. The minimum is 30 seconds.
pub(crate) fn parse_page_cursor(cursor: &str) -> Result<u32, String> {
    let page = cursor
        .parse::<u32>()
        .map_err(|_| "cursor must be a positive page number".to_owned())?;
    if page == 0 {
        return Err("cursor must be a positive page number".to_owned());
    }
    Ok(page)
}

pub(crate) fn parse_interval(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    let (number, factor) = if let Some(number) = value.strip_suffix('s') {
        (number, 1)
    } else if let Some(number) = value.strip_suffix('m') {
        (number, 60)
    } else if let Some(number) = value.strip_suffix('h') {
        (number, 3600)
    } else {
        return Err("interval must end in s, m or h, for example 30s or 15m".to_owned());
    };
    let number = number
        .parse::<u64>()
        .map_err(|_| "interval must be a whole number".to_owned())?;
    let seconds = number
        .checked_mul(factor)
        .ok_or_else(|| "interval is too large".to_owned())?;
    if seconds < 30 {
        return Err("interval must be at least 30s".to_owned());
    }
    Ok(Duration::from_secs(seconds))
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
            "--category",
            "Fahrräder & Zubehör",
            "--location",
            "Berlin",
            "--min-price",
            "12.50",
            "--ad-type",
            "wanted",
            "--poster-type",
            "private",
            "--picture-required",
            "--buy-now-only",
            "--include-top-ads",
            "--exclude",
            "defekt",
            "--min-rooms",
            "2,5",
            "--max-size",
            "120",
            "--pages",
            "2",
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
        assert_eq!(query.category.as_deref(), Some("Fahrräder & Zubehör"));
        assert_eq!(query.location.as_deref(), Some("Berlin"));
        assert_eq!(query.min_price_cents, Some(1_250));
        assert_eq!(query.ad_type, AdType::Wanted);
        assert_eq!(query.poster_type, Some(PosterType::Private));
        assert!(query.picture_required);
        assert!(query.buy_now_only);
        assert!(query.include_top_ads);
        assert_eq!(query.exclude, ["defekt"]);
        assert_eq!(query.min_rooms, Some(2.5));
        assert_eq!(query.max_size_m2, Some(120.0));
        assert_eq!(query.pages, 2);
        assert_eq!(query.sort, Some(SearchSort::PriceAscending));
        assert_eq!(query.limit, 10);
    }

    #[test]
    fn numeric_ids_are_validated() {
        assert_eq!(parse_numeric_id("123").expect("id"), "123");
        assert!(parse_numeric_id("12a").is_err());
        assert!(parse_numeric_id("").is_err());
    }

    #[test]
    fn reference_values_are_trimmed_and_must_not_be_empty() {
        assert_eq!(
            parse_reference_value("  Fahrräder & Zubehör  ").expect("value"),
            "Fahrräder & Zubehör"
        );
        assert!(parse_reference_value("  ").is_err());
    }

    #[test]
    fn location_name_and_numeric_id_conflict() {
        assert!(
            Cli::try_parse_from([
                "kleinanzeigen",
                "search",
                "test",
                "--location",
                "Berlin",
                "--location-id",
                "3331",
            ])
            .is_err()
        );
    }

    #[test]
    fn page_count_is_bounded() {
        assert!(Cli::try_parse_from(["kleinanzeigen", "search", "test", "--pages", "11"]).is_err());
    }

    #[test]
    fn intervals_parse_with_a_floor() {
        assert_eq!(
            parse_interval("30s").expect("interval"),
            Duration::from_secs(30)
        );
        assert_eq!(
            parse_interval("15m").expect("interval"),
            Duration::from_secs(900)
        );
        assert_eq!(
            parse_interval("1h").expect("interval"),
            Duration::from_secs(3600)
        );
        assert!(parse_interval("29s").is_err());
        assert!(parse_interval("15").is_err());
        assert!(parse_interval("5x").is_err());
    }

    #[test]
    fn gallery_arguments_parse_and_use_a_cursor() {
        let cli = Cli::try_parse_from([
            "kleinanzeigen",
            "gallery",
            "--pages",
            "2",
            "--limit",
            "5",
            "--cursor",
            "3",
        ])
        .expect("valid gallery");
        let Command::Gallery(args) = cli.command else {
            panic!("expected gallery command");
        };
        assert_eq!(args.pages, 2);
        assert_eq!(args.limit, 5);
        assert_eq!(args.start_page().expect("page"), 3);
    }

    #[test]
    fn stats_batch_requires_one_or_more_ids() {
        let cli = Cli::try_parse_from(["kleinanzeigen", "stats-batch", "1", "2"])
            .expect("valid stats batch");
        let Command::StatsBatch(args) = cli.command else {
            panic!("expected stats batch command");
        };
        assert_eq!(args.ids, ["1", "2"]);
        assert!(Cli::try_parse_from(["kleinanzeigen", "stats-batch"]).is_err());
    }

    #[test]
    fn watch_defaults_to_newest_first_and_one_page() {
        let cli = Cli::try_parse_from(["kleinanzeigen", "watch", "raspberry pi"])
            .expect("valid arguments");
        let Command::Watch(args) = cli.command else {
            panic!("expected watch command");
        };
        let query = args.to_query().expect("query");
        assert_eq!(query.pages, 1);
        assert!(query.cursor.is_none());
        assert_eq!(query.sort, Some(SearchSort::DateDescending));
        assert_eq!(args.poll_limit(), 0);
        assert!(!args.uses_state());
    }
}
