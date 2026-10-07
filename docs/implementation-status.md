# Implementation status

Living evidence tracker for the Businex platform rebuild mandate. Every requirement from the
mandate is listed with a status and concrete evidence. Status values:

- incomplete: not done yet
- implemented: code exists and compiles, but the acceptance behavior is not yet demonstrated
- tested: automated tests pass against the real behavior
- verified: demonstrated end to end (browser, real provider, production) with recorded evidence

Model used for this work: xiaomi-token-plan/mimo-v2.6-pro (no substitution). Any fallback will be
recorded here explicitly.

Last updated: 2026-10-07 (run: platform-rebuild-20261007).

## Phase 0 - Inspection, safety, planning

| ID | Requirement | Status | Evidence |
| --- | --- | --- | --- |
| P0.1 | Inspect current code, active runs, toolchains | verified | Repo inventory: Hono + better-sqlite3 + React web; toolchains: Node 24.18.0, npm 11.16.0, Docker 29.6.0, Compose v5.2.0; no Rust toolchain yet (rustup install started); no sudo; production containers businex-app-1, businex-tunnel-1, businex-redis-qa-20261007, businex-site |
| P0.2 | Inspect and integrate Redis commit 1c7f380, do not lose it | implemented | Commit 1c7f380 reviewed: transient Redis workspace event relay with sender dedup, message size cap, terminal-event exclusion, install prefix namespacing, degraded mode without credential logging, Compose hardening. Semantics carried into the new platform under events; docs/redis.md retained |
| P0.3 | Inspect production and take validated backups before migration | verified | Validated online SQLite backup (VACUUM INTO) from businex-app-1: integrity_check ok, table counts match, sha256 c60f3bd1...4221c03 recorded at /home/coffee/.openclaw/backups/businex-pre-platform-rebuild-20261007T1358Z/ (businex.db, validation.log, schema.sql). Production DB currently contains schema only, zero customer rows |
| P0.4 | Concrete phased implementation plan | implemented | docs/implementation-plan.md (phases 0-8 with deliverables and exit criteria) |
| P0.5 | Requirement checklist with actual evidence | implemented | This file |
| P0.6 | Keep owner-only Cloudflare Access and disabled production terminal | verified | deploy/compose.yaml keeps BUSINEX_TERMINAL_ENABLED=false; Access app 0e20b4fa-57d0-4486-a350-4fb94d5bcbfc unchanged this run |
| P0.7 | No paid service purchases | verified | Only free/open tooling used (rustup, Docker images already available locally or public) |
| P0.8 | Do not overwrite unrelated active work | verified | Only businex repo paths touched; unrelated containers/volumes (niovo, n8n, fonovo, kairos, qery, etc.) untouched |

## Target stack

| ID | Requirement | Status | Evidence |
| --- | --- | --- | --- |
| T1 | SolidJS + TypeScript + Vite web interface, reusable UI library, generated app frontend | incomplete | |
| T2 | Tauri 2 desktop shell macOS/Windows/Linux, scoped native bridges, signed updater setup | incomplete | |
| T3 | Responsive browser access for mobile plus real Android/iOS scaffolding with documented build requirements; no unbuilt native platform claimed tested | incomplete | |
| T4 | Rust + Axum + Tokio + Tower platform API and Rust durable workers, SQLx queries/migrations | incomplete | |
| T5 | PostgreSQL primary storage | incomplete | |
| T6 | Redis for ephemeral cache, rate limits, live events; durable work must not depend on Redis Pub/Sub | incomplete | |
| T7 | S3-compatible file and artifact storage | incomplete | |
| T8 | OIDC-compatible identity plus secure server sessions, local development login | incomplete | |
| T9 | Company/workspace memberships, RBAC and scoped action permissions for humans, agents and generated apps | incomplete | |
| T10 | PostgreSQL RLS for tenant records | incomplete | |
| T11 | Generated backend extensions: TypeScript on Node LTS in isolated containers with resource/time/network limits and scoped API credentials; no host Docker socket, no unrestricted secrets, no direct production DB | incomplete | |
| T12 | Typed TypeScript app SDK and OpenAPI API contracts | incomplete | |
| T13 | Versioned app manifests (permissions, data schemas, routes, schedules, dependencies), compiled immutable bundles, per-company installs and version history | incomplete | |
| T14 | Model adapters: OpenAI, Anthropic, Gemini, OpenAI-compatible/local, Xiaomi for builder where configured | incomplete | |
| T15 | Per-company model keys stored protected, streaming, model selection, budgets, token/cost tracking with explicit unknown prices | incomplete | |
| T16 | PostgreSQL durable queue: leases, cancellation, schedules, bounded retries, idempotency, uncertain external-outcome handling | incomplete | |
| T17 | OpenTelemetry, structured redacted logs, health/readiness, latency/job/model usage metrics | incomplete | |
| T18 | Cargo tests, Vitest, Playwright, container CI/CD, Docker Compose self-hosting, managed-cloud deployment templates and provisioning scripts with explicit secrets and backups; no unsupported one-click claims | incomplete | |

## Product workflows

| ID | Requirement | Status | Evidence |
| --- | --- | --- | --- |
| W1 | Business desktop and CRM, projects/tasks, documents, calendar, invoices, team communication, agent management, settings reimplemented in chosen stack; no React/Node platform API as primary production runtime | incomplete | |
| W2 | Central AI app builder: describe app, agent creates records/screens/permissions/workflows, builds isolated, tests, real preview, install under company approval policy; generated apps need no Rust core recompile | incomplete | |
| W3 | Schema-driven apps and genuine custom TypeScript-code apps with real executed backend logic | incomplete | |
| W4 | App library/launcher, per-company install, data CRUD, upgrades/version history, uninstall without silent data deletion, migration compatibility, app rollback | incomplete | |
| W5 | Role matrix owner/admin/manager/member/viewer, invitations, membership management, resource scopes, agent action grants, audit trail; cross-tenant and privilege-escalation denials | incomplete | |
| W6 | Always-on cloud agents with durable execution surviving client close and worker restart; tool action approvals, retries, cancellation, visible outcomes; no unrestricted self-editing | incomplete | |
| W7 | Self-improvement: versioned prompts/templates/workflows, persisted eval cases/results, candidate-vs-baseline gate, controlled promotion and rollback; app/core release pipeline with staging, tests, monitoring; desktop signed updater distinct from cloud deploy and DB migration recovery | incomplete | |
| W8 | Simple, nontechnical, fast UI: desktop/mobile layouts, lazy app loading, accessible inputs, keyboard navigation, recoverable errors; measured startup payload and interaction timings | incomplete | |

## Acceptance evidence

| ID | Requirement | Status | Evidence |
| --- | --- | --- | --- |
| A1 | Full chosen stack builds with meaningful unit/integration/e2e tests | incomplete | |
| A2 | Inventory app created from natural language with a real configured model: barcode field, low-stock workflow, warehouse/member vs manager permissions, CRUD of actual persisted records in the installed generated app | incomplete | |
| A3 | Second custom-code app with calculation/integration logic and executable isolated backend (not a fixed template) | incomplete | |
| A4 | Two companies, two roles: isolation proven and unauthorized actions rejected including inside generated apps | incomplete | |
| A5 | Durable job restart recovery and duplicate prevention, Redis interruption recovery, app version upgrade/rollback, file upload/download | incomplete | |
| A6 | Model selection and real provider execution verified; no fake model calls or synthetic builder results; precise blockers reported if credentials unavailable | incomplete | |
| A7 | Rendered browser tests of real desktop and mobile user paths; Chromium/Firefox/WebKit where available | incomplete | |
| A8 | Production backup/migration/deploy/health and authenticated core workflows; public site/demo preserved unless intentionally migrated | incomplete | |
| A9 | Native target build evidence where the host permits; macOS/Windows toolchain gates remain incomplete if not built; coordinate with PM for a Mac build host | incomplete | |

## Known gates and limitations

- No sudo on this host: system packages (for example webkit2gtk for Tauri Linux) cannot be installed with apt directly; Docker-based builds will be used where possible.
- No Rust toolchain preinstalled; rustup user-level install in progress (started 2026-10-07).
- macOS and Windows desktop builds require external toolchains; keep incomplete until built on a real host.
