# Access and operating decisions

This document records which access covers each function, what the client may
fetch, how often, what is stored, and what must never appear in logs. It is the
gate for the read path and the starting point for account features.

## Capability table

| Function | Access | Supported | Evidence | Fallback |
| --- | --- | --- | --- | --- |
| Detail read | Mobile CAPI `GET /api/ads/{id}.json` | yes | Synthetic `tests/fixtures/detail.json`; live check with ordinary HTTPS | Fail closed |
| Search read | Mobile CAPI `GET /api/ads.json` | yes | Synthetic `tests/fixtures/search.json`; live check with ordinary HTTPS | Fail closed |
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

The read path uses the app-level Basic credential and the app headers only. It
does not use a user account, cookies or browser automation. The credential is
supplied by the operator through `KLEINANZEIGEN_BASIC_USER` and
`KLEINANZEIGEN_BASIC_PW`; it is not part of this repository.

The mobile API is private and its app version is a moving dependency. The
default is `2026.25.0` and can be overridden with `KLEINANZEIGEN_APP_VERSION`.
Re-check the read path when the app or the transport changes.

## Operating decisions

- **Request budget:** one request per invocation, no concurrency and no
  automatic retries. A 25-second request timeout, a page size of at most 50 and
  an 8 MiB response cap. A numeric `Retry-After` header is reported as
  `retry_after_seconds`; the caller decides when to retry.
- **State paths:** none. Settings come from the environment. There is no cache,
  session directory or audit log yet. `.env.local` is local configuration, not
  application state.
- **Storage:** nothing is persisted. Responses are converted in memory and
  written to stdout.
- **Log redaction:** no `Authorization`, app install id, token, response body,
  contact data or coordinates in logs. In JSON mode logging is disabled. A
  request log line carries only the operation, HTTP status and duration.
- **Token storage:** not applicable to the read path; the app credential is a
  shared app value, not a user token. User sessions are not implemented. A later
  login must use the OS keyring or an explicitly protected file for headless
  use.
- **Data minimization:** exact coordinates, contact names, phone numbers and
  account identifiers are not emitted. The CLI exposes listing id, title,
  public URL, price, city, postal code, description, timestamps, status,
  category, public image URLs and labeled attributes.
- **Kill switch:** unset `KLEINANZEIGEN_BASIC_USER` and
  `KLEINANZEIGEN_BASIC_PW`. `get` and `search` then fail before any request with
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
