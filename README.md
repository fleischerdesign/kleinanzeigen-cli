# kleinanzeigen-cli

[![CI](https://github.com/fleischerdesign/kleinanzeigen-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/fleischerdesign/kleinanzeigen-cli/actions/workflows/ci.yml)

Unofficial, browser-free CLI for [Kleinanzeigen](https://www.kleinanzeigen.de). Built in Rust for agents and humans.

> **Status:** early skeleton. The CLI surface, domain model, provider boundary and Nix packaging are in place. The mobile-API provider is the next milestone.

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
direnv allow
cargo build --release
```

The binary is `target/release/kleinanzeigen`.

## Usage

```sh
kleinanzeigen doctor
kleinanzeigen doctor --json
kleinanzeigen --help
```

Planned commands:

```sh
kleinanzeigen search "raspberry pi" --max-price 80 --json
kleinanzeigen get 3399373623 --json
kleinanzeigen watch "fendt bianco" --interval 15m
kleinanzeigen login
kleinanzeigen ads list
kleinanzeigen draft create --from draft.json
kleinanzeigen draft publish <draft-id> --confirm
kleinanzeigen messages list --unread
```

## JSON contract

Success goes to stdout:

```json
{
  "status": "ok",
  "version": "0.1.0",
  "platform": "linux",
  "arch": "x86_64",
  "provider": "not configured",
  "notes": [
    "No browser or Chromium is required.",
    "The mobile-API provider is not implemented yet."
  ]
}
```

Errors go to stderr as structured JSON:

```json
{"error":{"code":"invalid_input","message":"invalid input: listing id must be a non-empty numeric string"}}
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

## Architecture

```
src/
├── main.rs        binary entry point, exit-code mapping
├── lib.rs         library root, module contract
├── cli.rs         clap surface and the shell contract
├── doctor.rs      local diagnostics
├── error.rs       typed errors and stable exit codes
├── output.rs      JSON and human-readable output
├── domain/
│   ├── listing.rs listing ids, prices and listings
│   └── query.rs   search requests
└── provider/
    └── mod.rs     Provider trait and search page
```

The provider is deliberately isolated: a mobile-API change must not leak into
the domain or the CLI. The concrete provider will live under
`provider::mobile_api` and implement `Provider`.

## Development

```sh
direnv allow
nix develop

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
nix flake check
```

The dev shell provides `cargo`, `rustc`, `rustfmt`, `clippy` and `rust-analyzer`.
`direnv` loads it automatically via `.envrc`.

## Roadmap

1. **Transport spike.** `rquest` with Chrome impersonation against the mobile API; one search request and one detail request, with real responses as fixtures.
2. **Read-only.** `search`, `get`, `watch`; typed models, SQLite cache, rate limiting.
3. **Auth.** Login, session token, `doctor` checks.
4. **Own ads.** List, pause, activate, delete, renew.
5. **Drafts and publishing.** JSON/YAML input, `--dry-run`, `--confirm <plan-id>`, audit log.
6. **Messages.** List and reply with explicit confirmation.
7. **MCP wrapper.** Optional, on the same CLI/JSON contract; no second transport.

## Research and attribution

The mobile API is private and undocumented. The following projects were used as
research references:

- [`monkrel/kleinanzeigen-api`](https://github.com/monkrel/kleinanzeigen-api) (MIT) — mobile JSON API endpoints, auth scheme and domain operations.
- [`its-me-prash/kleinanzeigen-reader`](https://github.com/its-me-prash/kleinanzeigen-reader) (MIT) — listing extraction, JSON-LD/GTM fields and mobile-API notes.
- [`DanielWTE/ebay-kleinanzeigen-api`](https://github.com/DanielWTE/ebay-kleinanzeigen-api) (MIT) — API schema and error semantics.
- [`Second-Hand-Friends/kleinanzeigen-bot`](https://github.com/Second-Hand-Friends/kleinanzeigen-bot) (AGPL-3.0) — listing states and YAML behavior as a domain reference only.

No AGPL code is included in this repository. If code is ported from an MIT
project, its copyright and license notice will be preserved.

## License

MIT. See [LICENSE](LICENSE).

## Disclaimer

Unofficial. Not affiliated with, authorized by, or endorsed by Kleinanzeigen.
Use at your own risk.
