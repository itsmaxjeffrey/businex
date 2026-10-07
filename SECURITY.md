# Security Policy

## Reporting a vulnerability

Please open a private security advisory on GitHub or email the maintainer. Do not file public
issues for vulnerabilities.

## Model

- Passwords are hashed with scrypt (per-user salt, constant-time comparison).
- Sessions are signed tokens with short lifetimes and refresh rotation.
- API keys are scoped (read/write per module), hashed at rest, and audited on use.
- Every mutation writes an audit entry (actor, action, entity, timestamp).
- Workspaces are strictly isolated: every query is scoped by workspace id.

## Scope

Businex runs on your infrastructure. You are responsible for who can reach it. Bind to
`127.0.0.1` by default and put authentication in front of any public deployment.
