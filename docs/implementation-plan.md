# Implementation plan - Businex platform rebuild

Phased plan for the mandate of 2026-10-07. Each phase has deliverables and exit criteria; work is
committed per coherent milestone and tracked in docs/implementation-status.md.

## Decisions (with reasons in plain language)

1. Replace the Node/React runtime with Rust (Axum) and SolidJS as mandated. Existing TypeScript domain
   routes are used as a behavior reference, not as the production runtime. This keeps one language on
   the server (Rust) and one in the browser (TypeScript/Solid), which makes the security model and the
   generated-app boundary simpler to reason about.
2. Keep the existing public site and demo (site/) untouched. Keep production (app.businex.app) running
   until the new stack passes its tests; cutover happens in a dedicated migration step with backups.
3. Redis stays an ephemeral service: live events, cache, rate limits. All durable work (jobs, agent
   runs, schedules) lives in PostgreSQL with leases. This is the rule from commit 1c7f380 carried
   forward: Redis Pub/Sub loses messages on disconnect, so it can never be the queue.
4. Generated apps run in isolated Node containers with strict CPU/memory/time/network limits and
   scoped short-lived credentials. They talk to the platform only through the typed SDK/API, never to
   the database or Docker socket. This is what makes third-party generated code safe to run.
5. Tenant isolation uses PostgreSQL Row Level Security in addition to application checks. Two
   independent layers mean a bug in one place does not leak another company's data.

## Phases

### Phase 0 - Inspection, safety, planning (done this run)
- Inventory repo, toolchains, production containers; inspect Redis commit 1c7f380.
- Validated production backup before any migration work.
- Deliverables: docs/implementation-plan.md, docs/implementation-status.md.
- Exit: backup validated (integrity + counts + checksum), plan reviewed by PM.

### Phase 1 - Platform core in Rust
- Cargo workspace: businex-core (domain), businex-db (SQLx + migrations + RLS), businex-queue
  (Postgres durable queue: leases, retries, idempotency, cancellation, schedules), businex-events
  (Redis live events, non-durable), businex-api (Axum + Tower: sessions, RBAC middleware, health,
  readiness), businex-worker (durable worker binary).
- PostgreSQL schema with per-tenant RLS policies; migration runner with versioned migrations.
- Dev environment: Docker Compose with PostgreSQL 17, Redis 7, MinIO (S3-compatible).
- Exit: cargo build + cargo test green against a real PostgreSQL; queue survives simulated worker
  restart and duplicate enqueue; RLS tests prove cross-tenant denial.

### Phase 2 - Identity, tenancy, RBAC
- OIDC-compatible login plus local development login; secure server sessions (rotating, hashed).
- Companies/workspaces, memberships, role matrix owner/admin/manager/member/viewer, invitations,
  resource scopes, agent action grants, audit trail, scoped action permissions enforced in one
  authorization layer used by humans, agents and generated apps.
- Exit: integration tests show role matrix behavior and privilege-escalation denial; audit entries
  for every mutation.

### Phase 3 - Web interface (SolidJS + Vite)
- Read the five required design skills before writing UI; apply them to the component library.
- packages/ui reusable library; business modules: desktop shell, CRM, projects/tasks, documents,
  calendar, invoices, team communication, agent management, settings; generated-app runtime UI.
- Accessibility: keyboard, focus order, loading/empty/error states, contrast, reduced motion.
- Exit: Vitest unit tests, Playwright e2e desktop + mobile viewport in Chromium/Firefox/WebKit
  where available; measured startup payload and interaction timings recorded in docs/performance.md.

### Phase 4 - App platform
- Versioned app manifests (permissions, data schemas, routes, schedules, dependencies); compiled
  immutable bundles; per-company installs with version history, upgrade, rollback, uninstall that
  preserves data. Schema-driven apps (records/screens/permissions/workflows) and custom TypeScript
  apps executing in isolated containers through the typed SDK.
- OpenAPI contract published and kept in sync with the API.
- Exit: a schema app and a custom-code app both installed, tested, upgraded and rolled back in tests.

### Phase 5 - AI app builder and model adapters
- Model adapters (OpenAI, Anthropic, Gemini, OpenAI-compatible/local, Xiaomi for builder where
  configured); per-company protected keys, streaming, selection, budgets, token/cost tracking with
  explicit unknown prices.
- Builder pipeline: describe app in natural language, generate manifest/schema/TS, build in the
  isolated environment, run tests, expose preview, install under company approval policy.
- Exit: inventory app created from natural language with a real configured model (A2) and a second
  custom-code app (A3), both executing real backend logic; real provider calls verified (A6).

### Phase 6 - Agents and self-improvement
- Always-on cloud agents on the durable queue: approvals for tool actions, retries, cancellation,
  visible outcomes; execution survives client close and worker restart.
- Self-improvement loop: versioned prompts/templates/workflows, evaluation cases persisted with
  results, candidate-vs-baseline gate, controlled promotion and rollback.
- Release pipeline for apps and core with staging, tests and monitoring; desktop signed updater
  configuration kept distinct from cloud deploy and database migration recovery.
- Exit: restart-recovery and duplicate-prevention tests (A5); promotion gate demonstrated.

### Phase 7 - Desktop and native targets
- Tauri 2 shell for macOS/Windows/Linux with scoped native bridges and signed updater setup
  (documented signing and update endpoints; keys never committed).
- Android/iOS target scaffolding with documented platform build requirements.
- Exit: Linux build where the host permits (Docker toolchain if system libs are unavailable);
  macOS/Windows stay incomplete until built on real hosts (coordinate with PM for a Mac host).

### Phase 8 - Operations, migration, docs
- Docker Compose self-hosting, managed-cloud deployment templates, provisioning scripts with
  explicit secrets and backup steps; OpenTelemetry metrics, structured redacted logs, health and
  readiness endpoints; operations docs.
- Production migration: backup, migrate data (SQLite to PostgreSQL), deploy, verify authenticated
  workflows behind the existing owner-only Cloudflare Access, keep terminal disabled.
- Exit: A8 verified; docs complete: architecture, deployment, app SDK, operations.
