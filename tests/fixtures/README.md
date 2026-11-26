# Fixtures

Fixtures in this directory are public artifacts. They must be synthetic. The
`search.json` and `detail.json` pair models the mobile API response shape with
invented listings, placeholder images and no live values.

- No raw captures from the live site or the app.
- No real names, email addresses, phone numbers, postal addresses, account IDs,
  cookies, tokens or app credentials.
- Invent ad IDs, prices, titles and locations; keep only the response structure.
- Replace images with placeholders; do not commit copyrighted photos.
- If a real response shape must be represented, rewrite the values and keep the
  keys.
- A fixture that cannot be sanitized is not committed.

The same rule applies to test logs, snapshots and documentation examples.
