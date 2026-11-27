//! Local no-account state: watch progress, price history and saved searches.
//!
//! The database is optional and local. It stores public listing data only; no
//! credentials, cookies or account data are written.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::domain::{Listing, ListingId, SearchQuery};
use crate::error::{Error, Result};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS listings (
    id TEXT PRIMARY KEY,
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    status TEXT,
    category TEXT,
    city TEXT,
    postal_code TEXT,
    rooms REAL,
    size_m2 REAL,
    last_price_cents INTEGER,
    currency TEXT,
    price_type TEXT
);
CREATE TABLE IF NOT EXISTS price_history (
    listing_id TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    amount_cents INTEGER NOT NULL,
    currency TEXT NOT NULL,
    price_type TEXT
);
CREATE INDEX IF NOT EXISTS price_history_listing_time
    ON price_history(listing_id, seen_at);
CREATE TABLE IF NOT EXISTS watch_seen (
    query_key TEXT NOT NULL,
    listing_id TEXT NOT NULL,
    first_seen INTEGER NOT NULL,
    PRIMARY KEY (query_key, listing_id)
);
CREATE TABLE IF NOT EXISTS saved_searches (
    name TEXT PRIMARY KEY,
    search_json TEXT NOT NULL,
    interval TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_run INTEGER
);
"#;

/// One recorded price point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PricePoint {
    /// Unix timestamp when the price was observed.
    pub seen_at: i64,
    /// Amount in euro cents.
    pub amount_cents: i64,
    /// ISO 4217 currency.
    pub currency: String,
    /// Provider price classification.
    pub price_type: Option<String>,
}

/// One stored saved search.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SavedSearch {
    /// User-chosen name.
    pub name: String,
    /// Stored query.
    pub query: SearchQuery,
    /// Watch interval, for example `15m`.
    pub interval: String,
    /// Unix creation timestamp.
    pub created_at: i64,
    /// Unix timestamp of the last watch run, if any.
    pub last_run: Option<i64>,
}

/// One local state database.
pub(crate) struct State {
    connection: Connection,
}

impl State {
    /// Opens the default state database for writing, creating it if needed.
    pub(crate) fn open_default() -> Result<Self> {
        Self::open_at(&state_path()?)
    }

    /// Opens the default state database read-only if it already exists.
    ///
    /// Inspecting state never creates, migrates, chmods or otherwise changes
    /// the database.
    pub(crate) fn open_existing_read_only() -> Result<Option<Self>> {
        let path = state_path()?;
        if !path.exists() {
            return Ok(None);
        }
        Self::open_read_only_at(&path).map(Some)
    }

    /// Opens the default state database for writing if it already exists.
    ///
    /// Mutations use this so that removing a saved search never creates an
    /// empty database.
    pub(crate) fn open_existing_writable() -> Result<Option<Self>> {
        let path = state_path()?;
        if !path.exists() {
            return Ok(None);
        }
        Self::open_existing_at(&path).map(Some)
    }

    /// Opens a state database at a given path, creating and initializing it.
    pub(crate) fn open_at(path: &Path) -> Result<Self> {
        Self::open_at_with_timeout(path, Duration::from_secs(5))
    }

    /// Opens a state database at a given path read-only.
    pub(crate) fn open_read_only_at(path: &Path) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| Error::Internal(format!("could not open state database: {error}")))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error("busy timeout"))?;
        require_current_schema(&connection)?;
        Ok(Self { connection })
    }

    fn open_existing_at(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .map_err(|error| Error::Internal(format!("could not open state database: {error}")))?;
        set_private_file(path)?;
        configure_writable(&connection, Duration::from_secs(5))?;
        require_current_schema(&connection)?;
        Ok(Self { connection })
    }

    fn open_at_with_timeout(path: &Path, timeout: Duration) -> Result<Self> {
        ensure_state_directory(path)?;
        create_private_file(path)?;

        let mut connection = Connection::open(path)
            .map_err(|error| Error::Internal(format!("could not open state database: {error}")))?;
        set_private_file(path)?;
        configure_writable(&connection, timeout)?;

        let version = schema_version(&connection)?;
        if version == 0 {
            initialize_schema(&mut connection)?;
        } else if version != SCHEMA_VERSION {
            return Err(unsupported_version(version));
        }

        Ok(Self { connection })
    }

    /// Returns the ids already seen for one watch key.
    pub(crate) fn seen_ids(&self, query_key: &str) -> Result<HashSet<ListingId>> {
        let mut statement = self
            .connection
            .prepare("SELECT listing_id FROM watch_seen WHERE query_key = ?1")
            .map_err(db_error("prepare seen"))?;
        let rows = statement
            .query_map([query_key], |row| row.get::<_, String>(0))
            .map_err(db_error("query seen"))?;
        let mut ids = HashSet::new();
        for row in rows {
            let id = row.map_err(db_error("read seen"))?;
            ids.insert(
                ListingId::new(id)
                    .map_err(|error| Error::Internal(format!("state listing id: {error}")))?,
            );
        }
        Ok(ids)
    }

    /// Records one page of listings and its watch bookkeeping atomically.
    ///
    /// `reset_seen` clears the stored seen set for `query_key` in the same
    /// transaction that records the new page, so a failed run cannot discard
    /// resume progress. `saved_name` updates the saved search's last-run time.
    pub(crate) fn record_page(
        &mut self,
        query_key: &str,
        listings: &[Listing],
        now: i64,
        reset_seen: bool,
        saved_name: Option<&str>,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction()
            .map_err(db_error("begin transaction"))?;

        if reset_seen {
            transaction
                .execute("DELETE FROM watch_seen WHERE query_key = ?1", [query_key])
                .map_err(db_error("clear seen"))?;
        }

        for listing in listings {
            record_listing_in(&transaction, listing, now)?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO watch_seen(query_key, listing_id, first_seen) \
                     VALUES (?1, ?2, ?3)",
                    params![query_key, listing.id.as_str(), now],
                )
                .map_err(db_error("mark seen"))?;
        }

        if let Some(name) = saved_name {
            transaction
                .execute(
                    "UPDATE saved_searches SET last_run = ?1 WHERE name = ?2",
                    params![now, name],
                )
                .map_err(db_error("mark saved run"))?;
        }

        transaction
            .commit()
            .map_err(db_error("commit transaction"))?;
        Ok(())
    }

    /// Returns the recorded price history for one listing.
    pub(crate) fn history(&self, id: &ListingId) -> Result<Vec<PricePoint>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT seen_at, amount_cents, currency, price_type \
                 FROM price_history WHERE listing_id = ?1 ORDER BY seen_at, rowid",
            )
            .map_err(db_error("prepare history"))?;
        let rows = statement
            .query_map([id.as_str()], |row| {
                Ok(PricePoint {
                    seen_at: row.get(0)?,
                    amount_cents: row.get(1)?,
                    currency: row.get(2)?,
                    price_type: row.get(3)?,
                })
            })
            .map_err(db_error("query history"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error("read history"))
    }

    /// Stores or replaces a saved search.
    pub(crate) fn saved_add(
        &self,
        name: &str,
        query: &SearchQuery,
        interval: &str,
        now: i64,
    ) -> Result<()> {
        let search_json = serde_json::to_string(query).map_err(|error| {
            Error::Internal(format!("could not serialize saved search: {error}"))
        })?;
        self.connection
            .execute(
                "INSERT INTO saved_searches(name, search_json, interval, created_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(name) DO UPDATE SET \
                    search_json = excluded.search_json, \
                    interval = excluded.interval",
                params![name, search_json, interval, now],
            )
            .map_err(db_error("save search"))?;
        Ok(())
    }

    /// Lists saved searches ordered by name.
    pub(crate) fn saved_list(&self) -> Result<Vec<SavedSearch>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT name, search_json, interval, created_at, last_run \
                 FROM saved_searches ORDER BY name",
            )
            .map_err(db_error("prepare saved list"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })
            .map_err(db_error("query saved list"))?;
        let mut out = Vec::new();
        for row in rows {
            let (name, search_json, interval, created_at, last_run) =
                row.map_err(db_error("read saved list"))?;
            let query: SearchQuery = serde_json::from_str(&search_json).map_err(|error| {
                Error::Internal(format!("could not parse saved search: {error}"))
            })?;
            query
                .validate()
                .map_err(|error| Error::Internal(format!("invalid saved search: {error}")))?;
            out.push(SavedSearch {
                name,
                query,
                interval,
                created_at,
                last_run,
            });
        }
        Ok(out)
    }

    /// Returns one saved search.
    pub(crate) fn saved_get(&self, name: &str) -> Result<Option<SavedSearch>> {
        let row = self
            .connection
            .query_row(
                "SELECT name, search_json, interval, created_at, last_run \
                 FROM saved_searches WHERE name = ?1",
                [name],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error("read saved search"))?;

        let Some((name, search_json, interval, created_at, last_run)) = row else {
            return Ok(None);
        };
        let query: SearchQuery = serde_json::from_str(&search_json)
            .map_err(|error| Error::Internal(format!("could not parse saved search: {error}")))?;
        query
            .validate()
            .map_err(|error| Error::Internal(format!("invalid saved search: {error}")))?;
        Ok(Some(SavedSearch {
            name,
            query,
            interval,
            created_at,
            last_run,
        }))
    }

    /// Removes one saved search.
    pub(crate) fn saved_remove(&self, name: &str) -> Result<bool> {
        let changed = self
            .connection
            .execute("DELETE FROM saved_searches WHERE name = ?1", [name])
            .map_err(db_error("remove saved search"))?;
        Ok(changed > 0)
    }

    /// Returns all price-history rows, optionally for one listing.
    pub(crate) fn export_history(
        &self,
        listing: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ExportRow>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT listing_id, seen_at, amount_cents, currency, price_type \
                 FROM price_history WHERE (?1 IS NULL OR listing_id = ?1) \
                 ORDER BY listing_id, seen_at, rowid LIMIT ?2",
            )
            .map_err(db_error("prepare export"))?;
        let rows = statement
            .query_map(params![listing, limit], |row| {
                Ok(ExportRow {
                    listing_id: row.get(0)?,
                    seen_at: row.get(1)?,
                    amount_cents: row.get(2)?,
                    currency: row.get(3)?,
                    price_type: row.get(4)?,
                })
            })
            .map_err(db_error("query export"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error("read export"))
    }

    /// Returns all stored listings, optionally for one id.
    pub(crate) fn export_listings(
        &self,
        listing: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ExportListing>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, first_seen, last_seen, title, url, status, category, \
                    city, postal_code, rooms, size_m2, last_price_cents, currency, \
                    price_type \
                 FROM listings WHERE (?1 IS NULL OR id = ?1) \
                 ORDER BY last_seen DESC, id LIMIT ?2",
            )
            .map_err(db_error("prepare listing export"))?;
        let rows = statement
            .query_map(params![listing, limit], |row| {
                Ok(ExportListing {
                    id: row.get(0)?,
                    first_seen: row.get(1)?,
                    last_seen: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    status: row.get(5)?,
                    category: row.get(6)?,
                    city: row.get(7)?,
                    postal_code: row.get(8)?,
                    rooms: row.get(9)?,
                    size_m2: row.get(10)?,
                    last_price_cents: row.get(11)?,
                    currency: row.get(12)?,
                    price_type: row.get(13)?,
                })
            })
            .map_err(db_error("query listing export"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error("read listing export"))
    }
}

/// One exported stored listing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportListing {
    /// Listing identifier.
    pub id: String,
    /// Unix timestamp of the first observation.
    pub first_seen: i64,
    /// Unix timestamp of the last observation.
    pub last_seen: i64,
    /// Listing title.
    pub title: String,
    /// Public listing URL.
    pub url: String,
    /// Provider ad status at the last observation.
    pub status: Option<String>,
    /// Category name at the last observation.
    pub category: Option<String>,
    /// City at the last observation.
    pub city: Option<String>,
    /// Postal code at the last observation.
    pub postal_code: Option<String>,
    /// Number of rooms, if the listing exposes one.
    pub rooms: Option<f64>,
    /// Living area in square metres, if the listing exposes one.
    pub size_m2: Option<f64>,
    /// Last observed price in euro cents.
    pub last_price_cents: Option<i64>,
    /// ISO 4217 currency of the last observed price.
    pub currency: Option<String>,
    /// Provider price classification of the last observed price.
    pub price_type: Option<String>,
}

fn record_listing_in(transaction: &Transaction<'_>, listing: &Listing, now: i64) -> Result<()> {
    let previous = transaction
        .query_row(
            "SELECT last_price_cents, currency, price_type FROM listings WHERE id = ?1",
            [listing.id.as_str()],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(db_error("read previous price"))?;

    let amount_cents = listing.price.as_ref().map(|price| price.amount_cents());
    let currency = listing.price.as_ref().map(|price| price.currency());
    let price_type = listing.price_type.as_deref();
    let category = listing
        .category
        .as_ref()
        .map(|category| category.name.as_str());
    let city = listing
        .location
        .as_ref()
        .and_then(|location| location.city.as_deref());
    let postal_code = listing
        .location
        .as_ref()
        .and_then(|location| location.postal_code.as_deref());

    transaction
        .execute(
            "INSERT INTO listings(
                id, first_seen, last_seen, title, url, status, category, city,
                postal_code, rooms, size_m2, last_price_cents, currency, price_type
             ) VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                last_seen = excluded.last_seen,
                title = excluded.title,
                url = excluded.url,
                status = excluded.status,
                category = excluded.category,
                city = excluded.city,
                postal_code = excluded.postal_code,
                rooms = excluded.rooms,
                size_m2 = excluded.size_m2,
                last_price_cents = excluded.last_price_cents,
                currency = excluded.currency,
                price_type = excluded.price_type",
            params![
                listing.id.as_str(),
                now,
                listing.title,
                listing.url.as_str(),
                listing.status,
                category,
                city,
                postal_code,
                listing.rooms,
                listing.size_m2,
                amount_cents,
                currency,
                price_type,
            ],
        )
        .map_err(db_error("upsert listing"))?;

    if let (Some(amount_cents), Some(currency)) = (amount_cents, currency) {
        let changed = previous
            .as_ref()
            .is_none_or(|(old_amount, old_currency, old_type)| {
                old_amount != &Some(amount_cents)
                    || old_currency.as_deref() != Some(currency)
                    || old_type.as_deref() != price_type
            });
        if changed {
            transaction
                .execute(
                    "INSERT INTO price_history(
                        listing_id, seen_at, amount_cents, currency, price_type
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![listing.id.as_str(), now, amount_cents, currency, price_type],
                )
                .map_err(db_error("insert price history"))?;
        }
    }

    Ok(())
}

/// One exported price-history row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportRow {
    /// Listing identifier.
    pub listing_id: String,
    /// Unix observation timestamp.
    pub seen_at: i64,
    /// Amount in euro cents.
    pub amount_cents: i64,
    /// ISO 4217 currency.
    pub currency: String,
    /// Provider price classification.
    pub price_type: Option<String>,
}

fn configure_writable(connection: &Connection, timeout: Duration) -> Result<()> {
    connection
        .busy_timeout(timeout)
        .map_err(db_error("busy timeout"))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(db_error("journal mode"))?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(db_error("foreign keys"))
}

fn schema_version(connection: &Connection) -> Result<i64> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_error("schema version"))
}

fn require_current_schema(connection: &Connection) -> Result<()> {
    let version = schema_version(connection)?;
    if version == 0 {
        return Err(Error::Internal(
            "state database is not initialized".to_owned(),
        ));
    }
    if version != SCHEMA_VERSION {
        return Err(unsupported_version(version));
    }
    Ok(())
}

fn initialize_schema(connection: &mut Connection) -> Result<()> {
    let transaction = connection
        .transaction()
        .map_err(db_error("begin schema transaction"))?;
    transaction
        .execute_batch(SCHEMA)
        .map_err(db_error("schema"))?;
    transaction
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(db_error("schema version write"))?;
    transaction
        .commit()
        .map_err(db_error("commit schema transaction"))
}

fn unsupported_version(version: i64) -> Error {
    Error::Internal(format!("unsupported state schema version {version}"))
}

fn ensure_state_directory(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() || parent.exists() {
        return Ok(());
    }
    fs::create_dir_all(parent)
        .map_err(|error| Error::Internal(format!("could not create state directory: {error}")))?;
    set_private_directory(parent)
}

fn db_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> Error {
    move |error| Error::Internal(format!("state {context}: {error}"))
}

fn state_path() -> Result<PathBuf> {
    if let Some(directory) = std::env::var_os("KLEINANZEIGEN_STATE_DIR") {
        return Ok(PathBuf::from(directory).join("state.db"));
    }

    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or_else(|| {
            Error::Internal(
                "could not determine a local state directory; set KLEINANZEIGEN_STATE_DIR"
                    .to_owned(),
            )
        })?;
    Ok(base.join("kleinanzeigen-cli").join("state.db"))
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| Error::Internal(format!("could not protect state directory: {error}")))
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(Error::Internal(format!(
            "could not create state database: {error}"
        ))),
    }
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(Error::Internal(format!(
            "could not create state database: {error}"
        ))),
    }
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| Error::Internal(format!("could not protect state database: {error}")))
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use url::Url;

    use super::*;
    use crate::domain::Price;

    fn path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("kleinanzeigen-state-{}", uuid::Uuid::new_v4()))
            .join("state.db")
    }

    fn listing(id: &str, amount_cents: i64) -> Listing {
        Listing {
            id: ListingId::new(id).expect("id"),
            title: "Synthetic".to_owned(),
            url: Url::parse("https://www.example.invalid/s-anzeige/1").expect("url"),
            price: Some(Price::new(amount_cents, "EUR", false).expect("price")),
            price_type: Some("SPECIFIED_AMOUNT".to_owned()),
            location: None,
            description: None,
            posted_at: None,
            status: None,
            ad_type: None,
            poster_type: None,
            category: None,
            seller_id: None,
            rooms: None,
            size_m2: None,
            images: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn state_roundtrip() {
        let path = path();
        let query = SearchQuery::new("synthetic");
        let id = ListingId::new("123").expect("id");
        {
            let mut state = State::open_at(&path).expect("state");
            state
                .saved_add("test", &query, "15m", 1)
                .expect("saved add");
            assert_eq!(
                state.saved_get("test").expect("saved").unwrap().name,
                "test"
            );

            state
                .record_page("key", &[listing("123", 100)], 10, false, None)
                .expect("record");
            assert!(state.seen_ids("key").expect("ids").contains(&id));
            state
                .record_page("key", &[listing("123", 100)], 11, false, None)
                .expect("record same");
            state
                .record_page("key", &[listing("123", 200)], 12, false, None)
                .expect("record new");
            assert_eq!(state.history(&id).expect("history").len(), 2);

            state
                .record_page("key", &[], 13, true, None)
                .expect("clear seen");
            assert!(state.seen_ids("key").expect("ids").is_empty());
            state
                .record_page("key", &[listing("123", 200)], 14, false, Some("test"))
                .expect("record saved run");
            assert!(state.seen_ids("key").expect("ids").contains(&id));
            assert_eq!(
                state.saved_get("test").expect("saved").unwrap().last_run,
                Some(14)
            );

            assert_eq!(state.export_history(None, 100).expect("export").len(), 2);
            assert_eq!(
                state
                    .export_history(Some("123"), 100)
                    .expect("export")
                    .len(),
                2
            );
            assert_eq!(state.export_listings(None, 100).expect("listings").len(), 1);
            assert_eq!(
                state
                    .export_listings(None, 1)
                    .expect("bounded listings")
                    .len(),
                1
            );
        }

        let before = fs::read(&path).expect("database bytes");
        {
            let read = State::open_read_only_at(&path).expect("read-only state");
            assert_eq!(read.history(&id).expect("read history").len(), 2);
            assert_eq!(
                read.saved_get("test")
                    .expect("read saved")
                    .expect("saved")
                    .last_run,
                Some(14)
            );
            assert!(
                read.saved_remove("test").is_err(),
                "read-only state must reject writes"
            );
        }
        assert_eq!(
            fs::read(&path).expect("database bytes after read"),
            before,
            "a read-only open must not change the database"
        );

        let state = State::open_at(&path).expect("state for removal");
        assert!(state.saved_remove("test").expect("remove"));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn read_only_open_does_not_change_permissions() {
        let path = path();
        let parent = path.parent().expect("parent");
        let mut state = State::open_at(&path).expect("state");
        state
            .record_page("key", &[listing("123", 100)], 1, false, None)
            .expect("record");
        drop(state);

        #[cfg(unix)]
        let before_mode = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(parent).expect("metadata").permissions().mode() & 0o777
        };
        #[cfg(unix)]
        assert_eq!(before_mode, 0o700);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o755))
                .expect("relax directory for the test");
        }
        #[cfg(unix)]
        let loose_mode = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(parent).expect("metadata").permissions().mode() & 0o777
        };

        let read = State::open_read_only_at(&path).expect("read-only state");
        drop(read);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let after_mode = fs::metadata(parent).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(
                after_mode, loose_mode,
                "a read-only open must not change directory permissions"
            );
        }

        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn read_only_open_rejects_an_uninitialized_database() {
        let path = path();
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(&path, b"").expect("empty database");
        let error = match State::open_read_only_at(&path) {
            Ok(_) => panic!("an uninitialized database must be rejected"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("not initialized"),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn rejects_an_unknown_schema_version() {
        let path = path();
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        {
            let connection = Connection::open(&path).expect("connection");
            connection
                .pragma_update(None, "user_version", 99)
                .expect("version");
        }

        let error = match State::open_at(&path) {
            Ok(_) => panic!("an unknown schema version must be rejected"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("schema version 99"),
            "unexpected error: {error}"
        );
        let error = match State::open_read_only_at(&path) {
            Ok(_) => panic!("a read-only version check must reject version 99"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("schema version 99"),
            "unexpected read-only error: {error}"
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn a_locked_database_times_out() {
        let path = path();
        let first = State::open_at_with_timeout(&path, Duration::from_millis(50))
            .expect("first connection");
        let mut second = State::open_at_with_timeout(&path, Duration::from_millis(50))
            .expect("second connection");

        first
            .connection
            .execute_batch("BEGIN IMMEDIATE")
            .expect("write lock");
        let error = second
            .record_page("key", &[listing("123", 100)], 1, false, None)
            .expect_err("second writer must not succeed");
        assert!(
            error.to_string().contains("locked"),
            "unexpected error: {error}"
        );
        first
            .connection
            .execute_batch("ROLLBACK")
            .expect("release lock");

        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
