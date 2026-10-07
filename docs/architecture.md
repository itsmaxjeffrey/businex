# Architecture

Businex is a local-first business operating layer. It deliberately avoids mandatory external
services: one Node.js process and one SQLite file are the whole core.

## Principles

1. **Local-first** — the core runs with zero infrastructure. Everything is a file you can back up.
2. **Fast** — SQLite in WAL mode with prepared statements, FTS5 for search, and a single event bus.
3. **Agents are first-class users** — the same REST API that powers the UI powers agents (MCP tools, API keys, audit log).
4. **Composable modules** — every module is a route group + a UI module registered in the desktop shell.

## Runtime topology

```text
apps/web (React + Vite)                — desktop shell + modules
   │  HTTP REST  ·  WebSocket (realtime, terminal PTY streams)
   ▼
apps/server (Hono on @hono/node-server)
   ├─ auth        sessions, API keys, RBAC
   ├─ modules     crm, projects, documents, calendar, invoices, channels, analytics
   ├─ realtime    WebSocket hub (event bus → subscriptions)
   ├─ terminal    node-pty sessions per workspace
   ├─ search      SQLite FTS5 + tag graph
   └─ integrations
        ├─ openclaw   gateway bridge (sessions, messages, automations)
        ├─ open-tag   channels/threads/tasks protocol bridge
        └─ mcp        MCP server exposing Businex tools
   ▼
apps/server/data/businex.db (SQLite WAL)  ·  uploads/  ·  .businex/
```

## Data model

Core entities: `organizations`, `workspaces`, `users`, `memberships`, `roles`, `teams`,
plus module tables (`contacts`, `companies`, `deals`, `projects`, `tasks`, `documents`,
`events`, `invoices`, `channels`, `messages`, `agent_sessions`, `audit_log`, `api_keys`,
`tags`, `entity_tags`). Every row carries `workspace_id`; every query is workspace-scoped.

Documents and messages are indexed in FTS5 virtual tables kept in sync by triggers.

## Realtime

One WebSocket endpoint (`/ws`) multiplexes subscriptions:

- `entity:<type>:<id>` and `collection:<type>` change events
- channel message streams
- terminal PTY streams (`terminal:<sessionId>`)
- agent activity events

## Security

See [SECURITY.md](../SECURITY.md). In short: scrypt passwords, signed session tokens, scoped
API keys hashed at rest, per-mutation audit entries, workspace isolation at query level.
