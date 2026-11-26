# Access and operating decisions

This document is the gate before the first provider commit. It records which
access covers each function, what the client may fetch, how often, what is
stored, and what must never appear in logs.

## Capability table

| Function | Access | Supported | Evidence | Fallback |
| --- | --- | --- | --- | --- |
| Detail read | | | | |
| Search read | | | | |
| Login and session | | | | |
| Own ads read | | | | |
| Pause / activate | | | | |
| Draft / publish | | | | |
| Messages read | | | | |
| Messages reply | | | | |
| Renew / delete | | | | |

`Supported` is `yes`, `no` or `unproven`. `Evidence` is a sanitized fixture, a
documented endpoint, or a reproducible command — not a claim from memory.
`Fallback` says what the CLI does when the access is unavailable: fail closed,
stay read-only, or omit the command.

## Operating decisions

- **Request budget:** requests per minute, burst, backoff, `Retry-After`.
- **State paths:** config, cache, session and audit directories.
- **Storage:** what is cached, for how long, and how it is invalidated.
- **Log redaction:** no cookies, tokens, message contents or personal data.
- **Token storage:** OS keyring where available; otherwise a protected file.
- **Kill switch:** how a user disables a provider without editing code.
- **Security review:** before real account tokens and before the first live
  mutation.

## Rules

- A function without a supported access row does not ship.
- A live mutation requires a bound plan, explicit confirmation and an audit
  entry.
- An unknown outcome is reported as unknown; it is never silently retried.
- The mobile API is private. Record its version dependency and re-check it when
  the app or the transport changes.
