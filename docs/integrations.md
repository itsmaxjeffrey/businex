# Integrations

Businex treats external systems as modules, not afterthoughts.

## OpenClaw

The OpenClaw bridge connects a Businex workspace to an OpenClaw gateway:

- **Agent console** — list configured agents and sessions, send messages, read replies.
- **Automations** — view and run scheduled jobs (cron/heartbeat) from the Businex UI.
- **Businex as a toolset** — an MCP server (`/mcp`) exposes Businex records as tools, and
  scoped API keys let an OpenClaw agent create tasks, contacts, invoices, and documents.
- **Agent teammates** — an OpenClaw-backed agent can be a member of a channel: mention it and
  it receives the thread context, does the work, and reports back.

Details: [integrations/openclaw.md](integrations/openclaw.md)

## open-tag

The channels module implements the open-tag collaboration model — channels, threads, DMs,
shared tasks, agent mentions, conversation turns — and can bridge to an open-tag daemon so
agent runtimes (Claude Code, Codex, Copilot, OpenClaw…) work as teammates.

Details: [integrations/open-tag.md](integrations/open-tag.md)
