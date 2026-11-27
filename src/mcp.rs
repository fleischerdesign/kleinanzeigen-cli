//! Minimal Model Context Protocol server over stdio.
//!
//! The server exposes the same read-only capabilities as the CLI and speaks
//! newline-delimited JSON-RPC 2.0. Stdout carries protocol messages only;
//! diagnostics belong on stderr. No tool mutates provider data or local state.

use std::sync::OnceLock;

use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::domain::{AdType, ListingId, PosterType, SearchQuery, SearchSort};
use crate::error::{Error, Result};
use crate::output::SCHEMA_VERSION;
use crate::provider::Provider;
use crate::provider::mobile_api::{AppApiSettings, MobileApiProvider};
use crate::state::State;

const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: [&str; 2] = ["2024-11-05", "2025-06-18"];
const ERROR_NOT_INITIALIZED: i64 = -32002;

#[derive(Default)]
struct Session {
    initialized: bool,
    ready: bool,
}

/// Serves MCP requests on stdin until the stream closes.
pub async fn run() -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    let mut session = Session::default();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| Error::Internal(format!("could not read stdin: {error}")))?
    {
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_request(request, &mut session).await,
            Err(error) => Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {error}"),
            )),
        };

        if let Some(response) = response {
            let mut encoded = serde_json::to_vec(&response)
                .map_err(|error| Error::Internal(format!("could not encode response: {error}")))?;
            encoded.push(b'\n');
            stdout
                .write_all(&encoded)
                .await
                .map_err(|error| Error::Internal(format!("could not write stdout: {error}")))?;
            stdout
                .flush()
                .await
                .map_err(|error| Error::Internal(format!("could not flush stdout: {error}")))?;
        }
    }

    Ok(())
}

enum Failure {
    Protocol(i64, String),
    Tool(Error),
}

async fn handle_request(request: Value, session: &mut Session) -> Option<Value> {
    let Some(object) = request.as_object() else {
        return Some(error_response(
            Value::Null,
            -32600,
            "invalid request: expected an object",
        ));
    };

    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error_response(
            request_id(object),
            -32600,
            "invalid request: jsonrpc must be 2.0",
        ));
    }

    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Some(error_response(
            request_id(object),
            -32600,
            "invalid request: method is required",
        ));
    };

    let id = match object.get("id") {
        None => {
            // MCP notifications must not receive a response. Only the
            // notification namespace is accepted without an id.
            if method.starts_with("notifications/") {
                if method == "notifications/initialized" && session.initialized {
                    session.ready = true;
                }
                return None;
            }
            return Some(error_response(
                Value::Null,
                -32600,
                "request id is required",
            ));
        }
        Some(Value::String(value)) => Value::String(value.clone()),
        Some(Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Value::Number(number.clone())
        }
        Some(_) => {
            return Some(error_response(
                Value::Null,
                -32600,
                "request id must be a string or integer",
            ));
        }
    };

    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Some(error_response(id, -32602, "params must be an object"));
    }

    match dispatch(method, &params, session).await {
        Ok(result) => Some(success_response(id, result)),
        Err(Failure::Protocol(code, message)) => Some(error_response(id, code, &message)),
        Err(Failure::Tool(error)) => Some(success_response(id, tool_error(&error))),
    }
}

fn request_id(object: &Map<String, Value>) -> Value {
    match object.get("id") {
        Some(Value::String(value)) => Value::String(value.clone()),
        Some(Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Value::Number(number.clone())
        }
        _ => Value::Null,
    }
}

async fn dispatch(
    method: &str,
    params: &Value,
    session: &mut Session,
) -> std::result::Result<Value, Failure> {
    match method {
        "initialize" => {
            if session.initialized {
                return Err(Failure::Protocol(
                    -32600,
                    "server is already initialized".to_owned(),
                ));
            }
            session.initialized = true;
            session.ready = false;
            Ok(initialize(params))
        }
        "ping" => Ok(json!({})),
        _ if !session.ready => Err(Failure::Protocol(
            ERROR_NOT_INITIALIZED,
            "server is not initialized".to_owned(),
        )),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(params).await,
        _ => Err(Failure::Protocol(
            -32601,
            format!("method not found: {method}"),
        )),
    }
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|version| SUPPORTED_VERSIONS.contains(version))
        .unwrap_or(PROTOCOL_VERSION);

    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": "kleinanzeigen",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Read-only public Kleinanzeigen access. No login, cookies, browser or mutation. Local state tools are read-only."
    })
}

async fn call_tool(params: &Value) -> std::result::Result<Value, Failure> {
    let Some(object) = params.as_object() else {
        return Err(Failure::Protocol(
            -32602,
            "tools/call params must be an object".to_owned(),
        ));
    };
    let Some(name) = object.get("name").and_then(Value::as_str) else {
        return Err(Failure::Protocol(
            -32602,
            "tools/call needs a tool name".to_owned(),
        ));
    };
    let arguments = match object.get("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(arguments)) => arguments.clone(),
        Some(_) => {
            return Err(Failure::Protocol(
                -32602,
                "tool arguments must be an object".to_owned(),
            ));
        }
    };
    if !is_known_tool(name) {
        return Err(Failure::Protocol(-32602, format!("unknown tool: {name}")));
    }

    let data = run_tool(name, &arguments).await.map_err(Failure::Tool)?;
    Ok(tool_success(&data))
}

fn is_known_tool(name: &str) -> bool {
    tools().iter().any(|tool| tool["name"] == name)
}

async fn run_tool(name: &str, args: &Map<String, Value>) -> Result<Value> {
    match name {
        "search" => {
            let query = parse_search(args, false)?;
            let page = provider()?.search(&query).await?;
            to_value(page)
        }
        "watch_once" => {
            let mut query = parse_search(args, true)?;
            if query.sort.is_none() {
                query.sort = Some(SearchSort::DateDescending);
            }
            let page = provider()?.search(&query).await?;
            to_value(page)
        }
        "get" => {
            let id = listing_id(args, "id")?;
            to_value(provider()?.listing(&id).await?)
        }
        "similar" => {
            let id = listing_id(args, "id")?;
            to_value(provider()?.similar(&id).await?)
        }
        "seller_other" => {
            let id = listing_id(args, "id")?;
            to_value(provider()?.seller_other_ads(&id).await?)
        }
        "gallery" => {
            let page = optional_u64(args, "page")?.unwrap_or(1);
            let pages = optional_u64(args, "pages")?.unwrap_or(1);
            let limit = optional_u64(args, "limit")?.unwrap_or(25);
            let page =
                u32::try_from(page).map_err(|_| Error::Input("page is too large".to_owned()))?;
            let pages =
                u32::try_from(pages).map_err(|_| Error::Input("pages is too large".to_owned()))?;
            let limit =
                u32::try_from(limit).map_err(|_| Error::Input("limit is too large".to_owned()))?;
            if page == 0 {
                return Err(Error::Input("page must be at least 1".to_owned()));
            }
            to_value(provider()?.gallery(page, pages, limit).await?)
        }
        "profile" => {
            let seller_id = required_string(args, "seller_id")?;
            to_value(provider()?.profile(&seller_id).await?)
        }
        "stats" => {
            let id = listing_id(args, "id")?;
            to_value(provider()?.stats(&id).await?)
        }
        "stats_batch" => {
            let ids = required_string_array(args, "ids")?;
            let ids = ids
                .iter()
                .map(|id| ListingId::new(id.as_str()))
                .collect::<Result<Vec<_>>>()?;
            to_value(provider()?.stats_batch(&ids).await?)
        }
        "images" => {
            let id = listing_id(args, "id")?;
            to_value(provider()?.images(&id).await?)
        }
        "categories" => {
            let mut entries = provider()?.categories().await?;
            if let Some(query) = optional_string(args, "query")?
                .map(|query| query.trim().to_lowercase())
                .filter(|query| !query.is_empty())
            {
                entries.retain(|entry| {
                    entry.name.to_lowercase().contains(&query)
                        || entry.path.to_lowercase().contains(&query)
                });
            }
            to_value(entries)
        }
        "locations" => {
            let query = required_string(args, "query")?;
            if query.trim().is_empty() {
                return Err(Error::Input("location query must not be empty".to_owned()));
            }
            let limit = optional_u64(args, "limit")?.unwrap_or(25);
            if limit == 0 || limit > 100 {
                return Err(Error::Input("limit must be between 1 and 100".to_owned()));
            }
            let mut entries = provider()?.locations(&query).await?;
            entries.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
            to_value(entries)
        }
        "search_metadata" => {
            let category = required_string(args, "category")?;
            to_value(provider()?.search_metadata(&category).await?)
        }
        "location" => {
            let id = required_string(args, "id")?;
            to_value(provider()?.location(&id).await?)
        }
        "flag_reasons" => to_value(provider()?.flag_reasons().await?),
        "saved_list" => {
            let saved = match State::open_existing_read_only()? {
                Some(state) => state.saved_list()?,
                None => Vec::new(),
            };
            to_value(saved)
        }
        "saved_show" => {
            let name = required_string(args, "name")?;
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::Input(
                    "saved search name must not be empty".to_owned(),
                ));
            }
            let saved = match State::open_existing_read_only()? {
                Some(state) => state.saved_get(name)?,
                None => None,
            };
            to_value(saved.ok_or_else(|| Error::NotFound(format!("saved search {name}")))?)
        }
        "history" => {
            let id = listing_id(args, "id")?;
            let points = match State::open_existing_read_only()? {
                Some(state) => state.history(&id)?,
                None => Vec::new(),
            };
            to_value(points)
        }
        _ => Err(Error::Internal(format!("unhandled tool: {name}"))),
    }
}

fn parse_search(args: &Map<String, Value>, single_page: bool) -> Result<SearchQuery> {
    let mut query = SearchQuery::new(required_string(args, "query")?);
    query.category = optional_string(args, "category")?.map(|value| value.trim().to_owned());
    query.location = optional_string(args, "location")?.map(|value| value.trim().to_owned());
    query.location_id = optional_string(args, "location_id")?.map(|value| value.trim().to_owned());
    query.radius_km = optional_u64(args, "radius_km")?
        .map(|value| {
            u32::try_from(value).map_err(|_| Error::Input("radius_km is too large".to_owned()))
        })
        .transpose()?;
    query.min_price_cents = optional_i64(args, "min_price_cents")?;
    query.max_price_cents = optional_i64(args, "max_price_cents")?;

    if let Some(value) = optional_string(args, "ad_type")? {
        query.ad_type = match value.as_str() {
            "offered" => AdType::Offered,
            "wanted" => AdType::Wanted,
            _ => {
                return Err(Error::Input("ad_type must be offered or wanted".to_owned()));
            }
        };
    }
    if let Some(value) = optional_string(args, "poster_type")? {
        query.poster_type = match value.as_str() {
            "private" => Some(PosterType::Private),
            "commercial" => Some(PosterType::Commercial),
            _ => {
                return Err(Error::Input(
                    "poster_type must be private or commercial".to_owned(),
                ));
            }
        };
    }

    query.picture_required = optional_bool(args, "picture_required")?.unwrap_or(false);
    query.buy_now_only = optional_bool(args, "buy_now_only")?.unwrap_or(false);
    query.include_top_ads = optional_bool(args, "include_top_ads")?.unwrap_or(false);

    if let Some(value) = optional_string(args, "sort")? {
        query.sort = Some(parse_sort(&value)?);
    }

    let limit = optional_u64(args, "limit")?.unwrap_or(25);
    query.limit =
        usize::try_from(limit).map_err(|_| Error::Input("limit is too large".to_owned()))?;
    let pages = if single_page {
        1
    } else {
        optional_u64(args, "pages")?.unwrap_or(1)
    };
    query.pages =
        u32::try_from(pages).map_err(|_| Error::Input("pages is too large".to_owned()))?;
    query.cursor = optional_string(args, "cursor")?.map(|value| value.trim().to_owned());
    query.exclude = optional_string_array(args, "exclude")?
        .unwrap_or_default()
        .into_iter()
        .map(|term| term.trim().to_owned())
        .collect();
    query.min_rooms = optional_f64(args, "min_rooms")?;
    query.max_rooms = optional_f64(args, "max_rooms")?;
    query.min_size_m2 = optional_f64(args, "min_size_m2")?;
    query.max_size_m2 = optional_f64(args, "max_size_m2")?;

    query.validate()?;
    Ok(query)
}

fn parse_sort(value: &str) -> Result<SearchSort> {
    match value {
        "date-descending" | "date_descending" => Ok(SearchSort::DateDescending),
        "price-ascending" | "price_ascending" => Ok(SearchSort::PriceAscending),
        "price-descending" | "price_descending" => Ok(SearchSort::PriceDescending),
        "distance-ascending" | "distance_ascending" => Ok(SearchSort::DistanceAscending),
        _ => Err(Error::Input(format!("unknown sort order: {value}"))),
    }
}

fn listing_id(args: &Map<String, Value>, name: &str) -> Result<ListingId> {
    ListingId::new(required_string(args, name)?)
}

fn provider() -> Result<MobileApiProvider> {
    MobileApiProvider::new(AppApiSettings::from_env()?)
}

fn to_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value)
        .map_err(|error| Error::Internal(format!("could not serialize tool result: {error}")))
}

fn required_string(args: &Map<String, Value>, name: &str) -> Result<String> {
    optional_string(args, name)?.ok_or_else(|| Error::Input(format!("{name} is required")))
}

fn optional_string(args: &Map<String, Value>, name: &str) -> Result<Option<String>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Error::Input(format!("{name} must be a string"))),
    }
}

fn optional_u64(args: &Map<String, Value>, name: &str) -> Result<Option<u64>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| Error::Input(format!("{name} must be a non-negative integer"))),
    }
}

fn optional_i64(args: &Map<String, Value>, name: &str) -> Result<Option<i64>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| Error::Input(format!("{name} must be an integer"))),
    }
}

fn optional_f64(args: &Map<String, Value>, name: &str) -> Result<Option<f64>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| Error::Input(format!("{name} must be a number"))),
    }
}

fn optional_bool(args: &Map<String, Value>, name: &str) -> Result<Option<bool>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| Error::Input(format!("{name} must be a boolean"))),
    }
}

fn optional_string_array(args: &Map<String, Value>, name: &str) -> Result<Option<Vec<String>>> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| Error::Input(format!("{name} must contain strings")))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(_) => Err(Error::Input(format!("{name} must be an array of strings"))),
    }
}

fn required_string_array(args: &Map<String, Value>, name: &str) -> Result<Vec<String>> {
    optional_string_array(args, name)?
        .filter(|values| !values.is_empty())
        .ok_or_else(|| Error::Input(format!("{name} must contain at least one value")))
}

fn tool_success(data: &Value) -> Value {
    let text = serde_json::to_string(&json!({
        "schema_version": SCHEMA_VERSION,
        "data": data
    }))
    .unwrap_or_else(|_| "{\"schema_version\":1,\"error\":{\"code\":\"internal\",\"message\":\"could not serialize result\"}}".to_owned());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

fn tool_error(error: &Error) -> Value {
    let mut body = json!({
        "code": error.code(),
        "message": error.to_string()
    });
    if let Some(retry_after) = error.retry_after_seconds() {
        body["retry_after_seconds"] = json!(retry_after);
    }
    let text = serde_json::to_string(&json!({
        "schema_version": SCHEMA_VERSION,
        "error": body
    }))
    .unwrap_or_else(|_| "{\"schema_version\":1,\"error\":{\"code\":\"internal\",\"message\":\"could not serialize error\"}}".to_owned());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": true
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn tools() -> &'static [Value] {
    static TOOLS: OnceLock<Vec<Value>> = OnceLock::new();
    TOOLS.get_or_init(build_tools).as_slice()
}

fn build_tools() -> Vec<Value> {
    let mut watch_schema = search_schema();
    if let Some(properties) = watch_schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        properties.remove("pages");
    }

    vec![
        tool(
            "search",
            "Search public listings. Prices are euro cents. Read-only.",
            search_schema(),
        ),
        tool(
            "get",
            "Fetch one public listing by numeric id.",
            id_schema("Numeric listing id."),
        ),
        tool(
            "similar",
            "List public ads similar to one listing. Fixed list, no pagination.",
            id_schema("Numeric listing id."),
        ),
        tool(
            "seller_other",
            "List other public ads from the same seller. Fixed list, no pagination.",
            id_schema("Numeric listing id."),
        ),
        tool(
            "gallery",
            "Fetch the public homepage gallery, optionally several pages.",
            json!({
                "type": "object",
                "properties": {
                    "page": { "type": "integer", "minimum": 1, "description": "First page, default 1." },
                    "pages": { "type": "integer", "minimum": 1, "maximum": 10, "description": "Pages to fetch, default 1." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Page size, default 25." }
                }
            }),
        ),
        tool(
            "profile",
            "Fetch one public seller profile by numeric seller id.",
            json!({
                "type": "object",
                "properties": {
                    "seller_id": { "type": "string", "description": "Numeric seller id." }
                },
                "required": ["seller_id"]
            }),
        ),
        tool(
            "stats",
            "Fetch public view and watchlist counters for one listing.",
            id_schema("Numeric listing id."),
        ),
        tool(
            "stats_batch",
            "Fetch public counters for 1 to 20 listings.",
            json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "maxItems": 20
                    }
                },
                "required": ["ids"]
            }),
        ),
        tool(
            "images",
            "Fetch all public image variants of one listing.",
            id_schema("Numeric listing id."),
        ),
        tool(
            "categories",
            "List the public category tree, optionally filtered by name or path.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Case-insensitive name or path filter." }
                }
            }),
        ),
        tool(
            "locations",
            "Resolve public locations by name or postal code.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Place name or postal code." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Maximum matches, default 25." }
                },
                "required": ["query"]
            }),
        ),
        tool(
            "search_metadata",
            "List public search filters for a category id or name.",
            json!({
                "type": "object",
                "properties": {
                    "category": { "type": "string", "description": "Category id or name." }
                },
                "required": ["category"]
            }),
        ),
        tool(
            "location",
            "Fetch one public location detail by numeric id.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Numeric location id." }
                },
                "required": ["id"]
            }),
        ),
        tool(
            "flag_reasons",
            "List the public reporting reasons. Does not report an ad.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "saved_list",
            "List locally saved searches. Read-only.",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "saved_show",
            "Show one locally saved search. Read-only.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Saved search name." }
                },
                "required": ["name"]
            }),
        ),
        tool(
            "history",
            "Read locally recorded price points for one listing. Read-only.",
            id_schema("Numeric listing id."),
        ),
        tool(
            "watch_once",
            "Fetch one bounded watch snapshot without storing anything. Dates sort newest first by default.",
            watch_schema,
        ),
    ]
}

fn id_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": description }
        },
        "required": ["id"]
    })
}

fn search_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Free-text query. An empty string browses a category."
            },
            "category": { "type": "string", "description": "Category id or name." },
            "location": { "type": "string", "description": "Location name or postal code." },
            "location_id": { "type": "string", "description": "Numeric location id." },
            "radius_km": { "type": "integer", "minimum": 1 },
            "min_price_cents": { "type": "integer", "minimum": 0 },
            "max_price_cents": { "type": "integer", "minimum": 0 },
            "ad_type": { "type": "string", "enum": ["offered", "wanted"] },
            "poster_type": { "type": "string", "enum": ["private", "commercial"] },
            "picture_required": { "type": "boolean" },
            "buy_now_only": { "type": "boolean" },
            "include_top_ads": { "type": "boolean" },
            "sort": {
                "type": "string",
                "enum": [
                    "date-descending",
                    "price-ascending",
                    "price-descending",
                    "distance-ascending"
                ]
            },
            "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
            "pages": { "type": "integer", "minimum": 1, "maximum": 10 },
            "cursor": { "type": "string", "description": "Opaque page cursor." },
            "exclude": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Client-side terms excluded from title and description."
            },
            "min_rooms": { "type": "number", "minimum": 0 },
            "max_rooms": { "type": "number", "minimum": 0 },
            "min_size_m2": { "type": "number", "minimum": 0 },
            "max_size_m2": { "type": "number", "minimum": 0 }
        },
        "required": ["query"]
    })
}
