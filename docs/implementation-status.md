# Implementation status

Living evidence tracker for the Businex platform rebuild mandate. Every requirement from the
mandate is listed with a status and concrete evidence. Status values:

- incomplete: not done yet
- implemented: code exists and compiles, but the acceptance behavior is not yet demonstrated
- tested: automated tests pass against the real behavior
- verified: demonstrated end to end (browser, real provider, production) with recorded evidence

Model used for this work: xiaomi-token-plan/mimo-v2.6-pro (no substitution). Any fallback will be
recorded here explicitly.

Last updated: 2026-10-07 (runs 1-20). Test tally: 103 passing across 26
targets (cargo test run20, zero failures): api 5, identity 3, core 13, rls 4,
events 7, queue 17, worker 9, models 45 (types/pricing 27 unit, adapter
contracts 11, durable store 7). Tests run against per-test disposable
databases on the protected dev stack (dev-test.env).

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
| T7 | S3-compatible file and artifact storage | incomplete | Tenant-scoped files metadata table with RLS (migration 0003) is in place; deploy/compose.dev.yaml declares a MinIO service, but there is no direct evidence of a running MinIO in this environment (image pull was denied), so it is not claimed as running. S3 client integration (upload/download paths, artifact storage) not yet implemented |
| T8 | OIDC-compatible identity plus secure server sessions, local development login | implemented | Server sessions (random 256-bit tokens, SHA-256 hashed at rest, HttpOnly SameSite=Strict cookies, revocation, expiry) with Argon2id passwords; local dev registration/login/logout/me (registration gated by config, off by default); verified membership lookup per request. Identity tests: register/login/me/logout flow, wrong-password rejection. OIDC provider integration (discovery, code flow, subject mapping) remains |
| T9 | Company/workspace memberships, RBAC and scoped action permissions for humans, agents and generated apps | tested | Company creation with atomic owner membership (SECURITY DEFINER function), invitations (single-use, email-bound tokens), role matrix enforced through real API calls. Two-companies/two-roles test proves cross-tenant read/invite denial and viewer/member privilege denial; scoped grants are explicit permission-resource pairs (see R2.6). Audit trail writes on membership actions. Agent action_grants table present; agent/app credential issuance lands with the app platform |
| T10 | PostgreSQL RLS for tenant records | tested | Migrations enable + FORCE RLS with company-context policies on every tenant table. Tested with the real non-superuser businex_app role (4 tests in businex-db/tests/rls.rs): cross-tenant read/insert/update/delete denied (WITH CHECK rejects foreign rows), pooled connection cannot leak company context between transactions, unset context sees nothing, runtime role cannot SET ROLE businex_service, service role spans tenants by design (BYPASSRLS, asserted in pg_roles). API-request-level enforcement follows in Phase 2 |
| T11 | Generated backend extensions: TypeScript on Node LTS in isolated containers with resource/time/network limits and scoped API credentials; no host Docker socket, no unrestricted secrets, no direct production DB | incomplete | |
| T12 | Typed TypeScript app SDK and OpenAPI API contracts | incomplete | |
| T13 | Versioned app manifests (permissions, data schemas, routes, schedules, dependencies), compiled immutable bundles, per-company installs and version history | incomplete | |
| T14 | Model adapters: OpenAI, Anthropic, Gemini, OpenAI-compatible/local, Xiaomi for builder where configured | implemented | businex-models adapters for all five protocol families (OpenAI chat-completions also drives compatible/local/Xiaomi endpoints; Anthropic Messages; Gemini generateContent). Contract-tested against a local mock provider over real HTTP (11 tests): complete + streaming, chunk-split/multibyte SSE reassembly, provider error frames, premature EOF, sanitized rejections. Real provider execution (A6) remains open until live credentials are exercised |
| T15 | Per-company model keys stored protected, streaming, model selection, budgets, token/cost tracking with explicit unknown prices | implemented | Keys sealed AES-256-GCM with random nonces and tenant+provider authenticated binding (moved ciphertext fails open; debug redacts material). Streaming via robust SSE parser. Pricing carries provider/model/currency/effective date/estimate flag; cost uses checked arithmetic; absent usage stays Option::None and yields unknown cost. Durable budgets in PostgreSQL (migration 0006) with atomic reserve/settle/release under a row lock: concurrent reservations cannot double-spend, settled reservations cannot replay, totals survive restart (store tests, 7). Model selection wiring into the builder lands with Phase 5 |
| T16 | PostgreSQL durable queue: leases, cancellation, schedules, bounded retries, idempotency, uncertain external-outcome handling | tested | businex-queue + businex-worker, 17 tests: claim/complete roundtrip with fencing token; duplicate enqueue prevented by idempotency key; worker-death lease expiry reclaim and exactly-once crash recovery (crash simulated in-process via claim-without-complete plus lease expiry; an actual OS worker process kill/restart is not yet directly evidenced); stale attempt fenced even with same worker id (expired lease cannot complete, fail or heartbeat); bounded retries recorded; cancellation truthful for queued and leased jobs; tenant-scoped cancellation denied across companies (RLS); schedules fire exactly once (interval + cron) with replay-proof idempotency keys; external effects recorded pending before the call and never auto-replayed (blind retry refused, reconciler path tested) |
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
| R5 | Queue tests: real worker restart, expiring leases, stale-completion fencing, scoped cancellation, duplicate enqueue/external action handling | partial | businex-queue/tests/queue.rs + tenant_scoped.rs + businex-worker/tests/worker.rs (17 tests) cover expiring leases, stale-completion fencing with identical worker id, scoped cancellation, and duplicate enqueue/external-action handling. Crash recovery is simulated in-process (claim-without-complete + lease expiry); a real OS worker process kill/restart probe remains outstanding |
| R6 | Status tracker accuracy: reviewed is distinct from implemented | implemented | This file rewritten with per-row evidence; test tally recorded above |
| R7 | Mac build host available (Xcode 26.6 via DEVELOPER_DIR), WebKit/Windows remain gates | implemented | Recorded in Known gates below; desktop build targets tracked honestly |
| R8 | Read and apply all five UI skills before UI work | incomplete | Skills staged in workspace/skills (excluded from commits via .git/info/exclude). Will be read in full before the first UI implementation run (Phase 3) |

## PM review 2 follow-ups (2026-10-07)

| ID | Review requirement | Status | Evidence |
| --- | --- | --- | --- |
| S1 | Test isolation: disposable databases or namespaced kinds with cleanup; suite passes repeatedly and under concurrency | tested | Every integration test now provisions its own disposable PostgreSQL database (businex-db TestDb: unique name, migrations applied, dropped on completion and on panic paths). Queue tests additionally namespace kinds per run. Two consecutive full runs green (run18: 58 tests; run19 repeated: 22 test targets ok, zero failures) |
| S2 | Tests require explicit BUSINEX_TEST_DATABASE_URL; no DATABASE_URL fallback | tested | TestDb refuses to run without BUSINEX_TEST_DATABASE_URL with an explicit message; all test helper fallbacks to DATABASE_URL removed. Tests are run via the protected dev-test.env (sourced, never printed) |
| S3 | Reconcile contract explicit and compiling | tested | JobHandler::reconcile returns Result<(), HandlerError>: Ok only when the handler resolved the listed effects (then handle runs fresh), error refuses the rerun. Documented in the trait; covered by pending_external_effects_block_blind_retry and the reconciler path test |
| S4 | Heartbeat stops on all exit paths; lease loss stops handler work before completion/side effects | tested | HeartbeatGuard aborts the heartbeat task on drop (every exit path including early DB errors). Lease loss/cancellation fires a watch channel that drops the handler future: no completion, no external effects afterwards. Tests: lease_loss_stops_handler_before_side_effects (stolen fencing token -> outcome abandoned, zero effects) and cancellation_stops_handler_before_side_effects (outcome canceled, zero effects, lease timestamp frozen) |
| S5 | Direct behavior evidence for role and queue tests, not only API health | tested | rls.rs (4 tests: catalog attributes, SET ROLE denial, CRUD isolation with real businex_app role, service-role span) and queue/worker suites (17 + 9) exercise behavior directly against the database |
| S6 | Action-specific grants: read grant must not inherit write/delete from issuer role | tested | Authorization::granted takes explicit ActionGrant permission-resource pairs; empty grants deny all. Test read_grant_does_not_imply_write_even_for_owner issues a read grant under an Owner role and proves write/delete are denied |
| S7 | Role hardening must verify catalog attributes and fail closed; managed services need explicit provisioning path | implemented | 0004 now raises warnings instead of silent swallowing. businex_db::current_role_attributes reads pg_roles/pg_auth_members; API startup refuses SUPERUSER/BYPASSRLS/service-membership roles, worker startup refuses non-BYPASSRLS/non-service roles. deploy/provision-roles.sql documents the managed-service provisioning path. Tests cover policy refusal cases |
| S8 | Use dev-test.env for integration tests; never print it | verified | Test runs source /home/coffee/.local/state/businex/dev-test.env (set -a; . file; set +a). The file is never printed and no URL/password appears in command arguments |

## Known gates and limitations

- No sudo on this host: system packages (for example webkit2gtk for Tauri Linux) cannot be installed with apt directly; Docker-based builds will be used where possible.
- Rust toolchain installed 2026-10-07 (rustup, user-level, stable 1.99.0).
- Linux WebKit (webkit2gtk-4.1) is missing on this host: Tauri Linux desktop build is a real gate until a container toolchain or system library path is arranged.
- macOS desktop build: PM-provided host available (Xcode 26.6, DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer). Build only when desktop code is ready and PM coordinates; not yet built.
- Windows desktop build: no toolchain available; remains incomplete until a Windows host is provided.
