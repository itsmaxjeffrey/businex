# Businex platform (Rust)

The platform core of Businex: an Axum API, durable PostgreSQL workers and the
SolidJS web client (added in Phase 3). This replaces the earlier Node/React
runtime as the primary production stack; the old code remains a behavior
reference until the migration completes.

## Crates

- businex-core: roles, permissions, scoped authorization, shared errors.
- businex-db: SQLx pool, migrations, row level security context helpers.
- businex-queue: durable jobs (leases, retries, idempotency, cancellation,
  schedules, external-effect tracking). Durable work lives in PostgreSQL.
- businex-events: transient live events over Redis Pub/Sub (cache, rate limits,
  notifications). Never used for durable work.
- businex-api: HTTP API (Axum + Tower), health and readiness.
- businex-worker: durable worker binary and handler framework.

## Development

Start the dev stack (PostgreSQL, Redis, MinIO):

    docker compose -f deploy/compose.dev.yaml up -d

Run the test suites (integration tests need a disposable PostgreSQL):

    export BUSINEX_TEST_DATABASE_URL=postgres://businex:businex_dev_only@127.0.0.1:55432/businex_dev
    export BUSINEX_TEST_REDIS_URL=redis://127.0.0.1:16391
    cargo test

Migration policy: additive, versioned SQL files in businex-db/migrations,
applied by both API and worker at startup. Destructive changes require a
documented migration window and a validated backup first.
