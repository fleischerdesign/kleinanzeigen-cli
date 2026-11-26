# Contributing

## Development

```sh
git clone https://github.com/fleischerdesign/kleinanzeigen-cli
cd kleinanzeigen-cli
direnv allow
nix develop
```

The dev shell provides the Rust toolchain, `clippy`, `rustfmt` and
`rust-analyzer`.

## Before a pull request

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
nix flake check
```

All four must pass. CI runs the same commands on Linux, macOS and Windows, plus
`nix flake check` on Linux.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/), scoped by
area:

```text
feat(provider): add mobile API search
fix(cli): map empty listing ids to exit code 2
docs(readme): document the JSON contract
```

## Code

- Keep the domain free of transport details.
- Put a new access path behind the `Provider` trait.
- Return typed `Error` variants, not strings, from library code.
- Do not use `unwrap` or `expect` in library code; tests may use them.
- No browser automation, no Chromium, no captcha evasion.
- No credentials, cookies or tokens in the repository, tests or logs.
- Fixtures are synthetic; see [`tests/fixtures/README.md`](tests/fixtures/README.md).
- Prefer small modules with explicit responsibilities.

## License

By contributing, you agree that your contribution is licensed under the MIT
license.
