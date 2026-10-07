# Redis integration

Redis connects realtime workspace events between Businex processes. The existing event bus delivers local notifications immediately and publishes non-terminal events to Redis. Other processes deliver those events through their workspace-filtered WebSocket connections. Sender IDs prevent self-delivery duplicates; received events are never republished. Installation prefixes separate environments sharing a Redis service.

This release does not use Redis for business records, session authority, caching, rate limits, or durable agent jobs. SQLite remains authoritative. Redis Pub/Sub is transient: disconnected subscribers miss events, and applications must fetch current records after reconnecting. Do not use Pub/Sub as a durable job queue or audit ledger.

## Configuration

- `BUSINEX_REDIS_URL`: optional `redis://` or `rediss://` connection URL. Omit it for local-only development. Use TLS (`rediss://`) for remote managed Redis.
- `BUSINEX_REDIS_PASSWORD_FILE`: optional password file, mounted at `/run/secrets/redis_password` in Compose.
- `BUSINEX_REDIS_PREFIX`: installation namespace, default `businex`. Use different prefixes for staging and production when sharing Redis.
- `BUSINEX_TEST_REDIS_URL`: disposable Redis URL used by the integration test. Never point tests at production.

`/api/health` includes `redis: connected`, `degraded`, or `disabled`. Redis failure does not stop local live notifications or database-backed requests. The client reconnects automatically, does not queue notifications offline, and logs a generic warning without connection credentials. Redis health is informational; the application's existing liveness health check remains independent.

## Production

Compose starts an authenticated Redis instance on an internal Docker network, with no host port. Only the application joins this network; the Cloudflare tunnel has no Redis access. Redis has a 128 MB data limit and uses `noeviction`. Persistence is disabled because this service carries ephemeral notifications only; enabling durable queues later requires reviewing storage, backups, queue behavior, and eviction settings.

Create a random hexadecimal password outside the repository and set `BUSINEX_REDIS_PASSWORD_FILE` in the private deployment environment to its absolute host path. The enclosing host directory should be mode 0700. The file must be readable by the container users through the Docker secret mount; a mode-0644 file inside that private directory works for file-backed Compose secrets. Never commit or print it. The Redis process reads a generated private config file, keeping the password out of process arguments.

## Validation

```sh
BUSINEX_TEST_REDIS_URL=redis://127.0.0.1:16390 npm test
npm run build
npm run smoke
```

The Redis test uses real publisher/subscriber connections, verifies exactly-once local/remote delivery for one published event, rejects malformed messages, isolates installation namespaces, and prevents terminal events from crossing processes. Redis Pub/Sub itself does not provide exactly-once durable delivery.

Shared events prepare the application for workers. They do not make the current SQLite deployment a multi-host highly available service. Authentication and authorization remain in the application; Redis credentials are trusted infrastructure credentials.
