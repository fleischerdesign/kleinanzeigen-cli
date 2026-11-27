# Security

Report vulnerabilities privately to **philipp@fleischer.design**. Do not open a
public issue for a vulnerability, a leaked token or account data.

## Secrets

- Never commit cookies, session tokens, passwords or API credentials.
- Use `.env.local` for local secrets; it is gitignored.
- The mobile API credential is an app-level Basic credential, not a user
  password. It still belongs in the environment, never in the repository.
- The CLI must not print credentials, cookies or message contents in logs.
- Test fixtures must be sanitized before they are committed.
- Public seller profiles are returned only by the explicit `profile` command;
  `search` and `get` omit contact names, phone numbers and exact coordinates.
- The MCP server exposes only read tools and never writes local state. It uses
  the same data minimization as the CLI.
- If a credential is exposed, rotate it at the source; deleting a commit is not
  enough.

## Scope

This is an unofficial client for a private API. The API can change without
notice. Reports about missing endpoints, rate limits or changed response shapes
are welcome, but they are not security vulnerabilities unless they expose user
data or credentials.
