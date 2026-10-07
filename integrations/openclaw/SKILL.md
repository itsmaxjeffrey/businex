---
name: businex
description: Operate Businex (Business OS) records from an agent — use when a task involves CRM contacts, companies, deals, projects, tasks, documents, invoices, calendar events, channels, or business search. Creates and updates business records through the Businex REST API or MCP tools with scoped API keys.
---

# Businex integration skill

Businex is a local-first business operating layer. Agents interact with it the
same way people do: through the REST API (scoped API keys) or the MCP server.

## Configuration

Two values are needed and both come from the Businex UI (Settings → API keys):

| Value | Where |
| --- | --- |
| `BUSINEX_URL` | Base URL of the Businex server, e.g. `http://127.0.0.1:8788` |
| `BUSINEX_API_KEY` | Scoped key, starts with `bnx_` (created in Settings → API keys) |

The key carries the workspace — no workspace header is needed for API keys.

## REST quick reference

Every call needs `Authorization: Bearer $BUSINEX_API_KEY`.

- `GET  $BUSINEX_URL/api/search?q=...` — full-text search across all records
- `GET|POST  /api/crm/contacts` · `/api/crm/companies` · `/api/crm/deals`
- `GET|POST  /api/projects` · `/api/projects/tasks`
- `POST /api/projects/tasks/:id/move` `{ "status": "in_progress", "position": 1 }`
- `GET|POST  /api/documents` — markdown knowledge base
- `GET|POST  /api/calendar/events`
- `GET|POST  /api/invoices` — line items, tax, totals computed server-side
- `GET|POST  /api/channels` · `POST /api/channels/:id/messages`
- `POST /api/messages/:id/task` — turn a message into a tracked task

## MCP tools

The same capabilities are exposed as MCP tools at `POST $BUSINEX_URL/api/mcp`
(JSON-RPC 2.0: `initialize`, `tools/list`, `tools/call`):

`businex.search` · `businex.contact.create` · `businex.task.create` ·
`businex.document.write` · `businex.invoice.create` · `businex.channel.post` ·
`businex.analytics.overview`

## Procedure

1. Confirm `BUSINEX_URL` and `BUSINEX_API_KEY` are set; if not, ask the user to
   create a key in Businex Settings → API keys.
2. Before creating duplicates, search first: `GET /api/search?q=<term>`.
3. Perform the work with the narrowest endpoint that fits; prefer MCP tools when
   the runtime supports MCP.
4. Verify the result by reading the record back (GET by id) when a mutation was
   performed.
5. Every call is audited in Businex (Settings → Audit log); mention the record
   id in the reply so humans can find it.

## Error handling

- `401` → key missing/invalid or revoked; ask for a new key.
- `403` → key lacks the scope (e.g. `projects:write`); ask for a broader key.
- `404` → wrong id or the record lives in another workspace.

Do not invent record ids; use ids returned by create calls or search results.
