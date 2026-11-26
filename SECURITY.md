# Security

Report vulnerabilities privately to **philipp@fleischer.design**. Do not open a
public issue for a vulnerability, a leaked token or account data.

## Secrets

- Never commit cookies, session tokens, passwords or API credentials.
- Use `.env.local` for local secrets; it is gitignored.
- The CLI must not print credentials, cookies or message contents in logs.
- Test fixtures must be sanitized before they are committed.

## Scope

This is an unofficial client for a private API. The API can change without
notice. Reports about missing endpoints, rate limits or changed response shapes
are welcome, but they are not security vulnerabilities unless they expose user
data or credentials.
