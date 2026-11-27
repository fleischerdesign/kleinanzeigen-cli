# Access and operating decisions

This document records which access covers each function, what the client may
fetch, how often, what is stored, and what must never appear in logs. It is the
gate for the read path and the starting point for account features.

## Capability table

| Function | Access | Supported | Evidence | Fallback |
| --- | --- | --- | --- | --- |
| Detail read | Mobile CAPI `GET /api/ads/{id}.json` | yes | Synthetic `tests/fixtures/detail.json`; live check | Fail closed |
| Search read | Mobile CAPI `GET /api/ads.json` | yes | Synthetic `tests/fixtures/search.json`; live check | Fail closed |
| Categories and locations | Mobile CAPI `categories.json`, `locations.json` | yes | Synthetic reference tests; live check | Fail closed |
| Search metadata | Mobile CAPI `ads/search-metadata/{category}.json` | yes | Synthetic metadata test; live check | Fail closed |
| Similar ads | Mobile CAPI `ads/similar/{id}.json` | yes, fixed list | Live check; the endpoint ignores `page`/`size` | Fail closed |
| Seller other ads | Mobile CAPI `ads/seller-other-ads/{id}.json` | yes, fixed list | Live check; the endpoint ignores `page`/`size` | Fail closed |
| Homepage gallery | Mobile CAPI `ads/hp-gallery.json` | yes, paginated | Live check; `page`/`size` work | Fail closed |
| Public seller profile | Mobile CAPI `users/public/{id}/profile.json` | yes | Synthetic profile test; live check | Fail closed |
| Public counters | Mobile CAPI view and watchlist counters | yes | Synthetic counter test; live check | Fail closed |
| Image variants | Mobile CAPI detail `pictures` | yes | Synthetic variants test; live check | Fail closed |
| Location detail | Mobile CAPI `locations/{id}.json` | yes | Live check | Fail closed |
| Public flag reasons | Mobile CAPI `v2/flags/ads/reasons` | yes | Live check | Fail closed |
| Ad-specific flags | Mobile CAPI `v2/flags/ads/{id}` | no | GET returns 405; reporting is not a read | Omit |
| Price and status filters | Search `priceType`, `adStatus` | no | Echoed but totals do not change | Omit |
| Dynamic attribute filters | Search `attr` | unproven | No accepted encoding found; `search-metadata` only | Omit |
| Commercial imprint | Gateway commercial-policy | no | Public profile returned only empty documents | Omit |
| Manufacturer catalogue | Gateway manufacturer-service | no | 401 without account | Omit |
| Loyalty top-trader badge | Gateway loyalty-service | no | 404 for a normal public seller | Omit |
| Category ad schema | Mobile CAPI `ads/metadata/{id}.json` | no | Internal read/write schema, not a public search filter | Omit |
| Watch | Polling the public search endpoint | yes | Unit and local-shell tests; live snapshot | Stop the stream |
| Login and session | Mobile Auth0 and account gateway | unproven | Not exercised | Stay read-only |
| Own ads read | User-scoped mobile CAPI | unproven | Requires a user token | Omit |
| Pause / activate | User-scoped mobile CAPI | unproven | Requires a user token | Omit |
| Draft / publish | User-scoped mobile CAPI | unproven | Requires a user token | Omit |
| Messages read | User-scoped mobile CAPI / gateway | unproven | Requires a user token | Omit |
| Messages reply | User-scoped mobile CAPI / gateway | unproven | Requires a user token | Omit |
| Renew / delete | User-scoped mobile CAPI | unproven | Requires a user token; renew may cost money | Omit |

`Supported` is `yes`, `no` or `unproven`. `Evidence` is a sanitized fixture, a
documented endpoint, or a reproducible command — not a claim from memory.
`Fallback` says what the CLI does when the access is unavailable: fail closed,
stay read-only, or omit the command.

The public read path uses the app-level Basic credential and the app headers
only. It does not use a user account, cookies or browser automation. The
credential is supplied by the operator through `KLEINANZEIGEN_BASIC_USER` and
`KLEINANZEIGEN_BASIC_PW`; it is not part of this repository.

Commercial imprint, ad-specific reporting, manufacturer catalogue and loyalty
badge endpoints are omitted rather than guessed at. The public `flag-reasons`
list is exposed; posting a report is not. `search-metadata` exposes category
filters; arbitrary dynamic `attr` query encoding is unverified and not sent.
`priceType` and `adStatus` are echoed by the provider but do not filter results,
so they are not exposed.

The mobile API is private and its app version is a moving dependency. The
default is `2026.25.0` and can be overridden with `KLEINANZEIGEN_APP_VERSION`.
Re-check the read path when the app or the transport changes.

## Operating decisions

- **Request budget:** search makes one request per page, at most 10 pages, plus
  at most one category and one location lookup. Pages are fetched sequentially
  with a 500 ms pause. `gallery` follows the same page budget. `watch` polls one
  search page per interval; the minimum interval is 30 seconds, and the seen set
  is in memory. `stats` makes two requests; `stats-batch` makes one watchlist
  request plus one view request per id. Reference, discovery, image, location
  and flag-reason commands make one request each, plus a category lookup when a
  metadata category is given by name. No concurrency and no automatic retries.
  A 25-second timeout, a page size of at most 50 and an 8 MiB response cap. A
  numeric `Retry-After` header is reported as `retry_after_seconds`; the caller
  decides when to retry.
- **State paths:** none. Settings come from the environment. There is no cache,
  session directory or audit log yet. `.env.local` is local configuration, not
  application state.
- **Storage:** nothing is persisted. Responses are converted in memory and
  written to stdout; `watch` keeps only the seen listing ids in memory.
- **Log redaction:** no `Authorization`, app install id, token, response body,
  contact data or coordinates in logs. In JSON mode logging is disabled. A
  request log line carries only the operation, HTTP status and duration.
- **Token storage:** not applicable to the read path; the app credential is a
  shared app value, not a user token. User sessions are not implemented. A later
  login must use the OS keyring or an explicitly protected file for headless
  use.
- **Data minimization:** search and get do not emit exact ad coordinates,
  contact names or phone numbers. They emit the public seller id, city, postal
  code, listing data and public image URLs. The explicit `profile` command
  returns the public seller profile, including the public contact name,
  member-since date, badges and counters. `location` returns the public location
  tree, including approximate location coordinates, not an ad's exact position.
  `flag-reasons` returns public reporting metadata. Nothing is cached.
- **Kill switch:** unset `KLEINANZEIGEN_BASIC_USER` and
  `KLEINANZEIGEN_BASIC_PW`. Public commands then fail before any request with
  exit code `2`. `doctor` reports `mobile-api (missing credentials)`.
- **Security review:** required before the first real user token and before the
  first live mutation, not only before messages.

## Rules

- A function without a supported access row does not ship.
- A live mutation requires a bound plan, explicit confirmation and an audit
  entry.
- An unknown outcome is reported as unknown; it is never silently retried.
- A provider response that does not match the expected shape is a format error.
  It is not skipped, guessed around or cached.
- The mobile API is private. Record its version dependency and re-check it when
  the app or the transport changes.
