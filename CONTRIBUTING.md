# Contributing to Businex

Thanks for helping build the business operating layer.

## Ground rules

1. **Local-first.** No feature may require a hosted service. Everything works on one machine.
2. **Fast by default.** Keep the server startup under a second and API responses in single-digit milliseconds. Measure before/after.
3. **Agents are users.** Anything a human can do through the UI must be reachable through the REST API and MCP tools, with scoped API keys and audit logging.
4. **Type safety.** Strict TypeScript end to end. Shared types live in `packages/shared`.

## Workflow

1. Fork and branch from `main` (`feat/...`, `fix/...`, `docs/...`).
2. Keep commits focused and imperative: `feat(crm): add deal stage drag-and-drop`.
3. Add or update tests for behavior changes.
4. Run `npm test` and `npm run smoke` before opening a PR.
5. Fill in the PR template: what changed, why, how it was verified.

## Project layout

- `apps/server` — Hono + SQLite backend, REST, WebSocket, PTY, integrations.
- `apps/web` — React desktop shell and modules.
- `packages/shared` — shared types and validation schemas.
- `docs` — architecture, feature, and integration documentation.

## Style

- Prefer small pure functions and explicit data flow.
- Errors: return structured `{ error: { code, message } }` payloads with correct HTTP status.
- UI: follow the design tokens in `apps/web/src/styles`; never hard-code hex colors in components.
