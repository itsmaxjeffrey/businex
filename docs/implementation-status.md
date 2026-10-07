# Implementation status

Living evidence tracker for the Businex platform rebuild mandate. Every requirement from the
mandate is listed with a status and concrete evidence. Status values:

- incomplete: not done yet
- implemented: code exists and compiles, but the acceptance behavior is not yet demonstrated
- tested: automated tests pass against the real behavior
- verified: demonstrated end to end (browser, real provider, production) with recorded evidence

Model used for this work: xiaomi-token-plan/mimo-v2.6-pro (no substitution). Any fallback will be
recorded here explicitly.

Last updated: 2026-10-07 (runs 1-11, Phase 1 core milestone). Test tally: 52 passing
(cargo test, /tmp/cargo-test-run11.log): api 5, core 12, rls 4, events 7, queue 17,
worker 7.

## Phase 0 - Inspection, safety, planning

| ID | Requirement | Status | Evidence |
| --- | --- | --- | --- |
| P0.1 | Inspect current code, active runs, toolchains | verified | Repo inventory: Hono + better-sqlite3 + React web (legacy, replaced by Phase 1+ work). Toolchains (refreshed 2026-10-07): Node 24.18.0, npm 11.16.0, Docker 29.6.0, Compose v5.2.0, Rust 1.99.0 (rustup user install). No sudo. Production containers businex-app-1, businex-tunnel-1, businex-redis-qa-20261007, businex-site untouched |
| P0.2 | Inspect and integrate Redis commit 1c7f380, do not lose it | tested | Commit 1c7f380 reviewed (transient relay: sender dedup, size cap, terminal-event exclusion, prefix namespacing, no credential logging; docs/redis.md retained). Semantics ported to businex-events (Rust) and tested against real Redis: 5 integration tests (exactly-once cross-process, terminal stays local, oversized dropped, prefix isolation, status lifecycle) plus 2 unit tests |
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
| T4 | Rust + Axum + Tokio + Tower platform API and Rust durable workers, SQLx queries/migrations | incomplete | Phase 1 foundation done and tested: six-crate workspace (core/db/queue/events/api/worker) builds with Rust 1.99; businex-worker executes jobs with lease heartbeat; businex-api serves health/readiness only so far. Authenticated business routes, OpenAPI contract and full worker handler set remain (Phases 2-6) |
| T5 | PostgreSQL primary storage | implemented | PostgreSQL 17 (businex-dev stack) as primary storage; SQLx runtime queries; 4 versioned migrations (0001 core/identity/audit, 0002 queue, 0003 files, 0004 lease fencing + role hardening) applied and exercised by the 52-test suite |
| T6 | Redis for ephemeral cache, rate limits, live events; durable work must not depend on Redis Pub/Sub | implemented | Live events in businex-events (tested, see P0.2). Durable work lives in the PostgreSQL queue only (T16): no queue, schedule or agent state touches Redis. Ephemeral cache and rate-limit uses not yet added |
| T7 | S3-compatible file and artifact storage | incomplete | Tenant-scoped files metadata table with RLS (migration 0003) is in place; MinIO runs in the dev compose stack. S3 client integration (upload/download paths, artifact storage) not yet implemented |
| T8 | OIDC-compatible identity plus secure server sessions, local development login | incomplete | |
| T9 | Company/workspace memberships, RBAC and scoped action permissions for humans, agents and generated apps | incomplete | |
| T10 | PostgreSQL RLS for tenant records | tested | Migrations enable + FORCE RLS with company-context policies on every tenant table. Tested with the real non-superuser businex_app role (4 tests in businex-db/tests/rls.rs): cross-tenant read/insert/update/delete denied (WITH CHECK rejects foreign rows), pooled connection cannot leak company context between transactions, unset context sees nothing, runtime role cannot SET ROLE businex_service, service role spans tenants by design (BYPASSRLS, asserted in pg_roles). API-request-level enforcement follows in Phase 2 |
| T11 | Generated backend extensions: TypeScript on Node LTS in isolated containers with resource/time/network limits and scoped API credentials; no host Docker socket, no unrestricted secrets, no direct production DB | incomplete | |
| T12 | Typed TypeScript app SDK and OpenAPI API contracts | incomplete | |
| T13 | Versioned app manifests (permissions, data schemas, routes, schedules, dependencies), compiled immutable bundles, per-company installs and version history | incomplete | |
| T14 | Model adapters: OpenAI, Anthropic, Gemini, OpenAI-compatible/local, Xiaomi for builder where configured | incomplete | |
| T15 | Per-company model keys stored protected, streaming, model selection, budgets, token/cost tracking with explicit unknown prices | incomplete | |
| T16 | PostgreSQL durable queue: leases, cancellation, schedules, bounded retries, idempotency, uncertain external-outcome handling | tested | businex-queue + businex-worker, 17 tests: claim/complete roundtrip with fencing token; duplicate enqueue prevented by idempotency key; worker-death lease expiry reclaim and restart recovery; stale attempt fenced even with same worker id (expired lease cannot complete, fail or heartbeat); bounded retries recorded; cancellation truthful for queued and leased jobs; tenant-scoped cancellation denied across companies (RLS); schedules fire exactly once (interval + cron) with replay-proof idempotency keys; external effects recorded pending before the call and never auto-replayed (blind retry refused, reconciler path tested) |
| T17 | OpenTelemetry, structured redacted logs, health/readiness, latency/job/model usage metrics | incomplete | Structured JSON logs via tracing-subscriber with documented redaction policy (never log secrets, tokens, provider errors, headers); /healthz liveness, /readyz dependency readiness, /api/health summary with request-id propagation and trace layer. OpenTelemetry metrics and latency/job/model usage metrics remain |
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

## PM review 1 follow-ups (2026-10-07)

| ID | Review requirement | Status | Evidence |
| --- | --- | --- | --- |
| R1 | Scoped credentials: empty/malformed grants must deny, unrestricted only after explicit grant decision | tested | ResourceScope is now explicit All vs Restricted (empty = deny everything, no empty-means-all). Authorization::member (verified membership) vs Authorization::granted (scoped agent/app). Core tests: empty_scoped_grant_denies_everything, scoped_grant_narrows_role_and_resources, scope_serializes_explicitly_and_rejects_unknown_modes |
| R2 | Authorization bound to authenticated actor + verified membership; tenant identity and System never client-asserted | implemented | Authorization now carries Actor + company_id + role + scope; doc contract states actor/company come from server-side session/membership lookup only. HTTP binding lands with Phase 2 auth routes |
| R3 | One canonical permission representation with strict roundtrip, no silent fallback | tested | Canonical dotted strings (records.write) everywhere; Permission serializes as the dotted string, parse rejects unknowns. Core tests: permission_roundtrips_as_dotted_string, unknown_permission_is_rejected_not_defaulted |
| R4 | Isolation tests with the real non-superuser runtime role, reads/inserts/updates/deletes + pooled context reuse | tested | businex-db/tests/rls.rs connects as businex_app (non-superuser, asserted in pg_roles) and covers all four operations plus same-connection context reuse and empty-context denial |
| R5 | Queue tests: real worker restart, expiring leases, stale-completion fencing, scoped cancellation, duplicate enqueue/external action handling | tested | businex-queue/tests/queue.rs + tenant_scoped.rs + businex-worker/tests/worker.rs (17 tests) cover all listed behaviors including stale attempt fenced with identical worker id |
| R6 | Status tracker accuracy: reviewed is distinct from implemented | implemented | This file rewritten with per-row evidence; test tally recorded above |
| R7 | Mac build host available (Xcode 26.6 via DEVELOPER_DIR), WebKit/Windows remain gates | implemented | Recorded in Known gates below; desktop build targets tracked honestly |
| R8 | Read and apply all five UI skills before UI work | incomplete | Skills staged in workspace/skills (excluded from commits via .git/info/exclude). Will be read in full before the first UI implementation run (Phase 3) |

## Known gates and limitations

- No sudo on this host: system packages (for example webkit2gtk for Tauri Linux) cannot be installed with apt directly; Docker-based builds will be used where possible.
- Rust toolchain installed 2026-10-07 (rustup, user-level, stable 1.99.0).
- Linux WebKit (webkit2gtk-4.1) is missing on this host: Tauri Linux desktop build is a real gate until a container toolchain or system library path is arranged.
- macOS desktop build: PM-provided host available (Xcode 26.6, DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer). Build only when desktop code is ready and PM coordinates; not yet built.
- Windows desktop build: no toolchain available; remains incomplete until a Windows host is provided.
