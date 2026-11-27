# kleinanzeigen-cli

[![CI](https://github.com/fleischerdesign/kleinanzeigen-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/fleischerdesign/kleinanzeigen-cli/actions/workflows/ci.yml)

Unofficial, browser-free CLI for [Kleinanzeigen](https://www.kleinanzeigen.de). Built in Rust for agents and humans.

> **Status:** read-only public API. Search, detail, reference data, discovery, metadata, counters and `watch` work without an account. Optional local SQLite state keeps watch progress, price history and saved searches; it holds public listing data only. There is no account login, no telemetry and no mutation command. The app credential is supplied by the operator and is not part of this repository.

## Why

- **No browser.** No Chromium, no Playwright, no browser automation.
- **Agent-first.** Stable JSON on stdout, structured errors on stderr, meaningful exit codes.
- **One binary.** Fast startup, no runtime, no Node or Python.
- **Reproducible.** `flake.nix`, `direnv`, pinned `Cargo.lock`.
- **Provider boundary.** The API transport is isolated from the CLI and the domain.

## Non-goals

- No browser automation.
- No official affiliation with Kleinanzeigen.
- No scraping at scale, no credential sharing, no captcha evasion.

## Install

With Nix:

```sh
nix profile install github:fleischerdesign/kleinanzeigen-cli
```

From source:

```sh
git clone https://github.com/fleischerdesign/kleinanzeigen-cli
cd kleinanzeigen-cli
cp .env.local.example .env.local
$EDITOR .env.local
direnv allow
cargo build --locked --release
```

The binary is `target/release/kleinanzeigen`.

## Provider configuration

The mobile API needs the app-level Basic credential that the Android app embeds.
It is not a user password and it is not distributed with this repository. Put it
in `.env.local`, which is gitignored:

```sh
KLEINANZEIGEN_BASIC_USER=...
KLEINANZEIGEN_BASIC_PW=...
```

`direnv` loads `.env.local` through `.envrc`; a plain shell needs the variables
exported. The credential can change when the app updates. If it does, the public
commands fail with exit code `4` and a message to update the two values.

Optional overrides:

- `KLEINANZEIGEN_BASE_URL` — default `https://api.kleinanzeigen.de`; plain
  `http` is accepted only for loopback tests.
- `KLEINANZEIGEN_APP_VERSION` — default `2026.25.0`; the version sent in the app
  headers.

No account login, cookie import or browser is used.

## Usage

```sh
kleinanzeigen doctor
kleinanzeigen doctor --json
kleinanzeigen --help

# Search one page.
kleinanzeigen search "raspberry pi" --max-price 80 --limit 10 --json

# Follow the cursor from the previous result.
kleinanzeigen search "raspberry pi" --cursor 2 --json

# Resolve a category name and a place name.
kleinanzeigen search "fahrrad" --category "Fahrräder & Zubehör" --location Berlin --radius 20 --json

# Fetch two pages at once.
kleinanzeigen search "fahrrad" --pages 2 --limit 25 --json

# Fetch one listing.
kleinanzeigen get 1234567890 --json

# Public reference data and discovery.
kleinanzeigen categories "Fahrräder" --json
kleinanzeigen locations Berlin --json
kleinanzeigen location 3331 --json
kleinanzeigen search-metadata "Fahrräder & Zubehör" --json
kleinanzeigen similar 1234567890 --json
kleinanzeigen seller-other 1234567890 --json
kleinanzeigen gallery --pages 2 --limit 20 --json
kleinanzeigen images 1234567890 --json
kleinanzeigen flag-reasons --json

# Public seller and counter data.
kleinanzeigen profile 123456 --json
kleinanzeigen stats 1234567890 --json
kleinanzeigen stats-batch 1234567890 1234567891 --json

# Poll new listings as one JSON event per line.
kleinanzeigen watch "fendt bianco" --interval 15m --json
kleinanzeigen watch "fendt bianco" --once --json

# Keep watch progress and price history, then resume later.
kleinanzeigen watch "fendt bianco" --store --interval 15m --json
kleinanzeigen watch "fendt bianco" --resume --once --json
kleinanzeigen history 1234567890 --json

# Saved searches and local exports.
kleinanzeigen saved add camper "fendt bianco" --max-price 20000 --interval 15m
kleinanzeigen saved list --json
kleinanzeigen watch --saved camper --store --json
kleinanzeigen export --kind history --format jsonl
kleinanzeigen export --kind listings --format csv --limit 100
```

Search filters:

- `--category <ID|NAME>` — numeric id or a category name.
- `--location <NAME|POSTCODE>` — place name or postal code; ambiguous
  postal codes list their numeric ids.
- `--location-id <ID>` — skip the lookup with a numeric location id.
- `--radius <KM>`, `--min-price <EUROS>`, `--max-price <EUROS>`
- `--ad-type <offered|wanted>` — default `offered`.
- `--poster-type <private|commercial>` — verified provider filter.
- `--picture-required`, `--buy-now-only`, `--include-top-ads`
- `--exclude <TERM>` — repeatable, filters title and description after the fetch.
- `--min-rooms <N>`, `--max-rooms <N>`, `--min-size <M2>`, `--max-size <M2>`
  — client-side filters over provider attributes; missing values are excluded.
- `--limit <1-50>`, `--pages <1-10>`, `--cursor <CURSOR>`
- `--sort <date-descending|price-ascending|price-descending|distance-ascending>`

Prices are decimal euros, not floats in the shell. Category and location names
cost one lookup request each; numeric ids skip it. Multi-page searches fetch
sequentially with a short delay and deduplicate listings by id.
`--include-top-ads` may add paid top ads beyond the configured page size.
Client-side filters affect `items`, not the provider's `total`.

The provider echoes `priceType` and `adStatus` but does not use them as filters;
the CLI deliberately does not expose them. The same applies to arbitrary dynamic
`attr` queries: `search-metadata` reports the filters, but their query encoding
is not verified. `similar` and `seller-other` are fixed lists; only the public
gallery paginates.

`watch` uses the same filters, polls one page per interval, prints a snapshot on
the first poll and only unseen listings afterwards, and keeps deduplication in
memory by default. The interval is at least `30s`; the default is `15m`.
`--max-polls` bounds the run; `--once` is a one-poll snapshot. A restart starts
with a new snapshot unless state is used.

`--store` persists seen ids and price points in the local SQLite state.
`--resume` starts from the stored seen set without replaying the snapshot, and
`--fresh` clears that set and starts a new snapshot. `--saved <name>` supplies
the query, filters and interval from a saved search; filters and `--interval`
cannot be combined with it. State is optional and local; delete the database to
reset it.

Planned, not implemented: `login`, own ads, drafts and publishing, messages,
MCP.

Planned, not implemented: `login`, own ads, drafts and publishing, messages,
MCP.

The public read surface covers search, detail, reference data, discovery,
metadata, public counters and `watch`. `similar` and `seller-other` are fixed
lists; only the gallery paginates. `priceType` and `adStatus` are echoed by the
provider but not applied, so they are not exposed as filters. Commercial
imprint, ad-specific reporting, manufacturer catalogue and loyalty badges are
intentionally not part of the CLI; the public `flag-reasons` list is exposed.
`search-metadata` reports category filters;
arbitrary dynamic `attr` queries are not sent because their encoding is
unverified.

## JSON contract

Success goes to stdout:

Doctor:

```json
{
  "schema_version": 1,
  "data": {
    "status": "ok",
    "version": "0.1.0",
    "platform": "linux",
    "arch": "x86_64",
    "provider": "mobile-api (configured)",
    "app_version": "2026.25.0",
    "notes": [
      "No browser or Chromium is required.",
      "Read-only: detail and search use the mobile API."
    ]
  }
}
```

One search result:

```json
{
  "schema_version": 1,
  "data": {
    "items": [
      {
        "id": "1234567890",
        "title": "Synthetic bike & trailer",
        "url": "https://www.example.invalid/s-anzeige/synthetic-bike/1234567890-87-12345",
        "price": { "amount_cents": 1250, "currency": "EUR", "negotiable": true },
        "price_type": "PLEASE_CONTACT",
        "location": { "city": "Teststadt", "postal_code": "12345" },
        "description": "A synthetic description.",
        "posted_at": "2026-11-26T23:48:37+01:00",
        "status": "ACTIVE",
        "ad_type": "OFFERED",
        "poster_type": "PRIVATE",
        "category": { "id": "87", "name": "Synthetic category" },
        "seller_id": null,
        "rooms": null,
        "size_m2": null,
        "images": ["https://img.example.invalid/synthetic-bike-large.jpg"],
        "attributes": [
          { "name": "synthetic.condition", "label": "Condition", "value": "Good" }
        ]
      }
    ],
    "total": 2,
    "page": 1,
    "size": 25,
    "pages_fetched": 1,
    "next_cursor": "2"
  }
}
```

`get` returns one listing object in `data`. `categories` and `locations` return
arrays in `data`; `search-metadata` returns an array of filters.

A public seller profile:

```json
{
  "schema_version": 1,
  "data": {
    "id": "123456",
    "name": "Synthetic Seller",
    "since": "2020-01-01",
    "poster_type": "PRIVATE",
    "initials": "SS",
    "historical_ads": 3,
    "online_ads": 2,
    "reply_speed": "3h",
    "badges": [{ "name": "rating", "level": 2, "value": null }],
    "secure_payment": true,
    "seller_account_type": "private"
  }
}
```

Public counters:

```json
{"schema_version":1,"data":{"id":"1234567890","views":7,"watchlist":2}}
```

`stats-batch` returns an array of the same objects in `data`. `images` returns
all public variants of one listing; `gallery` uses the same page shape as
`search`:

```json
{
  "schema_version": 1,
  "data": {
    "id": "1234567890",
    "variants": [
      { "rel": "large", "url": "https://img.example.invalid/large.jpg" },
      { "rel": "XXL", "url": "https://img.example.invalid/xxl.jpg" }
    ]
  }
}
```

`location` returns a public location detail; `flag-reasons` returns an array of
public reporting reasons.

`watch` emits newline-delimited JSON, one object per line:

```json
{"schema_version":1,"event":"snapshot","data":{"id":"1234567890"}}
{"schema_version":1,"event":"new","data":{"id":"1234567891"}}
```

The `data` object is the same listing shape as a search item. A stream failure
stops the stream and writes the normal error object to stderr.

Local state results use the same success envelope. `history` returns an array of
price points:

```json
{
  "schema_version": 1,
  "data": [
    { "seen_at": 1764246000, "amount_cents": 1250, "currency": "EUR", "price_type": "PLEASE_CONTACT" },
    { "seen_at": 1764332400, "amount_cents": 1100, "currency": "EUR", "price_type": "SPECIFIED_AMOUNT" }
  ]
}
```

`saved add` and `saved show` return one saved search; `saved list` returns an
array. `saved remove` returns `{"name":"camper","removed":true}`:

```json
{
  "schema_version": 1,
  "data": {
    "name": "camper",
    "query": { "query": "fendt bianco", "max_price_cents": 2000000, "limit": 25, "pages": 1 },
    "interval": "15m",
    "created_at": 1764246000,
    "last_run": null
  }
}
```

`export` is a streaming command: `--format jsonl` writes one object per line
without the success envelope, and `--format csv` writes a header and rows.
`--kind history` is the default and exports price points; `--kind listings`
exports stored listings with their last observed values. `--listing` restricts
either dataset to one id and `--limit` bounds the row count.

```json
{"listing_id":"1234567890","seen_at":1764246000,"amount_cents":1250,"currency":"EUR","price_type":"PLEASE_CONTACT"}
```

Errors go to stderr as structured JSON:

```json
{"schema_version":1,"error":{"code":"invalid_input","message":"invalid input: listing id must be a non-empty numeric string"}}
```

A temporary failure may include the provider's retry hint:

```json
{"schema_version":1,"error":{"code":"temporary","message":"temporary failure: provider rate limit reached (HTTP 429)","retry_after_seconds":30}}
```

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Internal error |
| `2` | Invalid input |
| `3` | Not found |
| `4` | Authentication or access denied |
| `5` | Temporary failure / rate limit |
| `6` | Unexpected response format |
| `7` | Mutation outcome unknown |

### Machine contract

- `--json` writes exactly one JSON object to stdout per command result.
- `watch --json` is the exception: it writes one event object per line, each with `schema_version`, `event` and `data`. `export` likewise writes bare rows, not an envelope.
- On failure, stderr contains exactly one JSON error object; logs are suppressed in JSON mode.
- `--help` and `--version` are plaintext and exit `0`; they are not command results.
- Every object carries `schema_version`. It changes only when the machine contract changes, independent of the binary version.
- Search pages carry `total`, `page`, `size`, `pages_fetched` and `next_cursor`. The cursor is opaque: pass it back as `--cursor`.
- With `--pages > 1`, `items` is the deduplicated union of the fetched pages; `size` remains the per-page size and `pages_fetched` says how many pages were actually fetched.
- Client-side filters (`--exclude`, rooms, size) affect `items`; `total` remains the provider's total.
- `posted_at` is an RFC 3339 string or `null`. `rooms` and `size_m2` are numbers or `null`.
- `seller_id` is the public numeric seller identifier; use it with `profile`.
- `gallery` uses the search-page shape; `images`, `stats-batch`, `location` and `flag-reasons` use their dedicated objects or arrays in `data`.
- `retry_after_seconds` appears only when the provider sent a numeric `Retry-After` header.

## Supported platforms

| Surface | Targets | CI |
| --- | --- | --- |
| Nix package | `x86_64-linux`, `aarch64-linux` | `x86_64-linux` built by `nix flake check`; `aarch64-linux` declared, not built in CI |
| Cargo binary | Linux, macOS, Windows | tested on `ubuntu-latest`, `macos-latest`, `windows-latest` |
| Rust | 1.85+ | MSRV job on 1.85 |

## Architecture

```
src/
├── main.rs        binary entry point, exit-code mapping
├── lib.rs         library root, module contract
├── cli.rs         clap surface and the shell contract
├── commands/      search, discovery, reference, local state and watch commands
├── doctor.rs      local diagnostics
├── error.rs       typed errors and stable exit codes
├── output.rs      JSON, NDJSON and human-readable output
├── state.rs       local SQLite: watch progress, price history, saved searches
├── domain/
│   ├── image.rs   public image variants
│   ├── listing.rs listing ids, prices, locations, categories, listings
│   ├── profile.rs public seller profiles and badges
│   ├── query.rs   search requests and sort order
│   ├── reference.rs categories, locations and search filters
│   └── stats.rs   public listing counters
└── provider/
    ├── mod.rs     Provider trait, search page and listing list
    └── mobile_api/
        ├── mod.rs        endpoint and HTTP orchestration
        ├── config.rs     environment settings and app Basic auth
        ├── dto.rs        transport DTOs
        ├── convert.rs    DTO-to-domain conversion and validation
        ├── reference.rs  category and location name resolution
        ├── html.rs       conservative text normalization
        └── transport.rs  reqwest transport boundary
```

The provider is deliberately isolated: a mobile-API change must not leak into
the domain or the CLI. The `Provider` trait is read-only; account mutations will
be a separate capability.

Domain types validate their invariants (`ListingId`, `Price`); providers
deserialize their own DTOs and convert explicitly. A malformed provider
response is a format error, not a silently skipped listing. The CLI JSON is an
output contract, not a serialization of internal structs.

## Data and privacy

- No browser, no cookies and no account login for reads.
- The app-level Basic credential is supplied through `.env.local` or the
  environment and is never committed.
- No telemetry and no remote cache. `watch` is stateless by default. With
  `--store`, `--resume` or `--fresh`, the CLI keeps a local SQLite database at
  `${XDG_STATE_HOME:-$HOME/.local/state}/kleinanzeigen-cli/state.db`; set
  `KLEINANZEIGEN_STATE_DIR` to choose another directory. The directory is
  `0700`, the database `0600`.
- The state database holds public listing fields, watch seen ids, price points
  and saved searches. It holds no credentials, cookies, tokens or raw response
  bodies. Delete the file to reset all local state.
- Listing output includes city, postal code, the public seller id and public
  image URLs. Exact ad coordinates, contact names and phone numbers are not
  emitted by search or get. The explicit `profile` command returns the public
  seller profile, including the public contact name, member-since date, badges
  and counters. The explicit `location` command returns the public location
  tree, including approximate location coordinates, not an ad's exact position.
- Logs never contain `Authorization`, the app install id, tokens or response
  bodies. JSON mode suppresses logs entirely.
- Search: one request per page, at most 10 pages, plus at most one category and
  one location lookup; pages are fetched sequentially with a 500 ms pause.
- `gallery`: one request per page, at most 10 pages, 500 ms pause.
- `watch`: one search request per poll, with name lookups when needed, at an
  interval of at least 30 seconds; one page per poll.
- `stats` makes two requests; `stats-batch` makes one watchlist request plus one
  view request per id. `categories`, `locations`, `location`, `similar`,
  `seller-other`, `profile`, `images`, `flag-reasons` and `search-metadata` make
  one request each, plus a category lookup for a metadata name.
- No automatic retries, a 25-second timeout, a maximum page size of 50 and an
  8 MiB response cap. A numeric `Retry-After` header is surfaced as
  `retry_after_seconds`.

## Development

```sh
direnv allow
nix develop

cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
nix flake check
```

The dev shell provides `cargo`, `rustc`, `rustfmt`, `clippy` and `rust-analyzer`.
`direnv` loads it automatically via `.envrc`.

## Roadmap

The roadmap is ordered by evidence, not by feature count. Each step has an
abort condition; a later step does not start on an unproven earlier one.
The public read surface and local no-account state are implemented: search,
detail, reference data, discovery, metadata, counters, `watch`, price history,
saved searches and export. The account milestones are not.

### 1. Access decision and transport spike

Record in [`docs/access.md`](docs/access.md) which access covers each function.
Prove one detail read over the chosen mobile-API transport; add a search read
only if the same transport supports it. Convert the response into a domain
object and derive a sanitized, synthetic fixture. Before the first provider
commit, decide config/state paths, request budget and log redaction.

**Abort:** the transport cannot produce a stable, interpretable detail read.
Do not build the rest on it.

**Risk:** the mobile API is private and can change with app updates. Record
that dependency explicitly; do not present it as a public contract.

**Not included:** login, live mutations, SQLite.

### 2. Usable read release

`get` first, then `search` with limit and cursor if the transport supports both.
Versioned JSON, stable errors, bounded reads, `Retry-After`. Publish the CLI,
synthetic tests and Nix package with this scope.

**Abort:** `get` is not reliably interpretable. If only `search` is unstable,
ship `get` alone.

**Not included:** SQLite, account features, release automation. `watch` is
implemented with a bounded polling budget.

### 3. Session and own ads read

Build a supported login and one account read operation as one slice. Before
that, decide token expiry, refresh/logout and account binding. Use the OS
keyring or an explicitly protected secret file for headless use; `doctor`
reports status, never secrets.

**Abort:** no safe session lifecycle (captcha/MFA, token storage) — stay
read-only.

**Not included:** mutations, browser cookie import, raw credential capture.

### 4. First mutation and local drafts

Build the plan envelope — account, target, change — and require `--dry-run`,
`--confirm <plan-id>` and a redacted audit log **before** the first write.
Start with pause and activate. Local JSON drafts can land in parallel; publish
only after this path, with its own preview.

**Abort:** the confirmed plan cannot be bound to the exact mutation, or an
unknown outcome cannot be handled safely. Never retry blindly after a timeout
without an idempotency guarantee.

**Not included:** delete, paid renew, YAML, messages.

### 5. Extensions as needed

Read messages first; replies are separately confirmed. `watch` is implemented
with an interval floor, an explicit poll limit and opt-in local state for resume,
price history and saved searches. Renew and delete deliberately separate. The
stdio MCP wrapper over the stable read contract is the next no-account
extension.

**Abort:** unclear cost, permission, load budget or concrete user need for the
individual feature.

**Not included:** an obligation to implement every optional feature.

## Fixture policy

Fixtures in `tests/fixtures/` are public artifacts and must be synthetic. No
raw captures, no real names, addresses, account ids, cookies, tokens or app
credentials. Invented listings and placeholder images only. See
[`tests/fixtures/README.md`](tests/fixtures/README.md).

## Research and attribution

The mobile API is private and undocumented. The following projects were used as
research references:

- [`monkrel/kleinanzeigen-api`](https://github.com/monkrel/kleinanzeigen-api) (MIT) — mobile JSON API endpoints, auth scheme and domain operations.
- [`its-me-prash/kleinanzeigen-reader`](https://github.com/its-me-prash/kleinanzeigen-reader) (MIT) — listing extraction, JSON-LD/GTM fields and mobile-API notes.
- [`DanielWTE/ebay-kleinanzeigen-api`](https://github.com/DanielWTE/ebay-kleinanzeigen-api) (MIT) — API schema and error semantics.
- [`Second-Hand-Friends/kleinanzeigen-bot`](https://github.com/Second-Hand-Friends/kleinanzeigen-bot) (AGPL-3.0) — listing states and YAML behavior as a domain reference only.

The transport was verified with ordinary HTTPS and the app headers; no Chrome
TLS impersonation is required. `monkrel` uses impersonation, but this client does
not, and it does not include the app credential.

No AGPL code is included in this repository. If code is ported from an MIT
project, its copyright and license notice will be preserved.

## License

MIT. See [LICENSE](LICENSE).

## Disclaimer

Unofficial. Not affiliated with, authorized by, or endorsed by Kleinanzeigen.
Use at your own risk.
