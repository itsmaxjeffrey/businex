# Production deployment

The production image serves the real frontend, REST API, and WebSocket endpoint from one origin. SQLite, uploads, and the session signing secret persist in a dedicated `/data` volume. The public website demo remains separate.

## Build and verify

```sh
npm test
npm run build
npm run smoke
docker build -t businex:production .
```

The smoke test uses a disposable database. Never point it at live customer data.

## Private deployment

`deploy/compose.yaml` starts an unprivileged application container and a dedicated Cloudflare Tunnel. The application listens only on host loopback, with no public application port. Provision a Cloudflare Access application for `app.businex.app`, restricted to the invited owner email, before connecting its tunnel or DNS record. Configure the tunnel origin as `http://app:8788`.

Store `BUSINEX_REGISTRATION_EMAILS` and `TUNNEL_TOKEN` in a mode-0600 environment file outside the repository. Do not commit credentials. Use:

```sh
docker compose --env-file /path/to/private.env -f deploy/compose.yaml up -d --build
```

Keep the data volume when upgrading. Back up the live SQLite database using SQLite's backup API, together with uploads and `secret.key`. Copying an open SQLite file without its WAL is not a complete backup. Restore to an isolated instance before relying on a backup.

## Production boundaries

- `BUSINEX_WEB_ORIGIN` specifies the permitted browser origin for CORS and WebSocket connections. Foreign-origin mutations are rejected.
- Session cookies are HttpOnly, SameSite=Lax, and Secure in production.
- `BUSINEX_REGISTRATION_EMAILS` limits account creation to a comma-separated invitation list.
- Terminal execution is disabled in production. Enabling it with `BUSINEX_TERMINAL_ENABLED=true` grants shell access inside the application runtime and requires a separate access review.
- Password recovery does not return reset credentials. Email delivery is not yet configured; the recovery endpoint always returns the same acknowledgement. Logged-in users can change their password using their current password.
- Scoped API keys require `realtime:read` (or `*`) for WebSocket access. Terminal operations additionally require terminal scopes and an enabled terminal.
- OpenClaw and open-tag must be configured per workspace; they are not automatically connected by deployment.

Do not expose the first deployment as a public signup service without reviewing tenant isolation, integration network access, rate limiting, email verification, and account recovery.
