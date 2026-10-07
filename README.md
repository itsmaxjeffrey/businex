# Businex — Business OS

**Give your business and its agents a place to work.**

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Node](https://img.shields.io/badge/Node-%3E%3D22-339933.svg)](https://nodejs.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-Strict-3178C6.svg)](https://typescriptlang.org)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

Businex is an open-source, local-first **operating layer for running a business** — with its humans
*and* its AI agents working side by side. It combines a local Node.js core, a desktop shell in the browser,
first-class agent collaboration through **OpenClaw** and **open-tag**, and the full operational
module map out of the box.

## What you get

- **A real desktop environment** — draggable, resizable, persistent module windows, dock, virtual desktops, and a global command palette (Ctrl/Cmd+K).
- **Fast local core** — one Node.js process, SQLite (WAL) with FTS5 full-text search, zero external services required. Starts in under a second.
- **Operational modules** — Dashboard, CRM (contacts · companies · deals pipeline), Projects & Tasks (kanban), Documents & Knowledge, Calendar, Invoices, Channels, Agents, Terminal, Analytics, Settings.
- **Human + agent collaboration** — open-tag style channels, threads, DMs and shared tasks where AI agents are persistent teammates with memory.
- **OpenClaw integration** — connect a gateway, run agent sessions, expose Businex as tools (MCP + REST API keys), and let agents operate the business records.
- **A real terminal** — xterm.js + PTY, tabs, per-workspace shell, streaming output.
- **Multi-workspace** — organizations, workspaces, roles, teams, invitations, audit log.

## Quick start

```bash
git clone https://github.com/itsmaxjeffrey/businex.git
cd businex
npm install
npm run dev
```

Open `http://localhost:5199`, register, and you get a clean workspace with the full module map.

## Architecture

```text
Browser desktop shell (React + Vite + Tailwind)  :5199
        |  REST + WebSocket (realtime, terminal PTY)
Businex server (Hono + TypeScript)               :8788
        |
SQLite (WAL, FTS5)  ·  attachments/  ·  .businex/
        |
Integrations: OpenClaw gateway · open-tag bridge · MCP endpoint
```

See [docs/architecture.md](docs/architecture.md) for details.

## Modules

| Module | What it does |
| --- | --- |
| Dashboard | KPIs, pipeline, upcoming work, agent activity |
| CRM | Contacts, companies, deals pipeline, activity timeline |
| Projects | Projects, kanban tasks, sprints, assignments |
| Documents | Markdown knowledge base with tags, backlinks, full-text search |
| Calendar | Events, scheduling, reminders |
| Invoices | Invoices, line items, status tracking, PDF export |
| Channels | open-tag style channels, threads, DMs, agent teammates |
| Agents | OpenClaw sessions, automations, agent console |
| Terminal | Real PTY terminals with tabs |
| Settings | Workspace, members, roles, teams, API keys, integrations |

## Integrations

- **OpenClaw** — see [docs/integrations/openclaw.md](docs/integrations/openclaw.md).
- **open-tag** — see [docs/integrations/open-tag.md](docs/integrations/open-tag.md).
- **MCP** — Businex exposes an MCP server so any agent runtime can use your business records as tools.

## Configuration

Everything works with zero configuration. Environment variables (all optional):

| Variable | Default | Purpose |
| --- | --- | --- |
| `BUSINEX_HOST` | `127.0.0.1` | API bind address |
| `BUSINEX_PORT` | `8788` | API port |
| `BUSINEX_WEB_ORIGIN` | `http://localhost:5199` | Allowed web origin |
| `BUSINEX_DATA_DIR` | `apps/server/data` | Database, secrets, uploads |

## Agent integration

- **OpenClaw skill** — install [integrations/openclaw/SKILL.md](integrations/openclaw/SKILL.md) into your
  OpenClaw agent to let it operate Businex records (search, tasks, contacts, invoices, documents, channels).
- **MCP** — point any MCP-capable runtime at `POST /api/mcp` with a scoped API key.
- **open-tag** — bridge agent execution to an open-tag daemon
  ([integrations/open-tag/README.md](integrations/open-tag/README.md)).

Create API keys in **Settings → API keys**; keys are scoped per module and audited.

## Development

```bash
npm run dev        # server + web together
npm run build      # production build
npm test           # unit + integration tests
npm run smoke      # end-to-end smoke test against a running server
```

## Performance

Modules load on demand, windows retain their state when minimized, and production assets use
compression and fingerprinted caching. The first optimization pass reduced the default desktop's
startup JavaScript by approximately 55%. See [measurements and verification](docs/performance.md).

## Production hosting

The working app can be deployed with persistent SQLite storage using the included Docker image
and [deployment guide](docs/deployment.md). The first hosted instance uses private access;
the public website demo remains separate. Production disables terminal execution by default,
restricts browser origins, and never exposes password recovery tokens.

## License

Apache 2.0 — see [LICENSE](LICENSE).
