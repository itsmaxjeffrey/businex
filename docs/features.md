# Feature map

What Businex ships today, and what is next.

## Desktop shell
- [x] Draggable, resizable, persistent windows (position/size/z-order survive reload)
- [x] Dock with running windows and module launcher
- [x] Global command palette (Ctrl/Cmd+K): modules, records, actions, search
- [x] Workspace switcher (multi-workspace)
- [x] Dark + light themes with design tokens

## Core platform
- [x] Register / login / sessions / change + reset password
- [x] Organizations, workspaces, members, roles, teams, invitations
- [x] Audit log for every mutation
- [x] Scoped API keys for agents and integrations
- [x] Full-text search (FTS5) across documents, messages, records

## Modules
- [x] Dashboard (KPIs, pipeline, work in flight, activity feed)
- [x] CRM: contacts, companies, deals pipeline (drag-and-drop), activities
- [x] Projects & tasks: kanban, drag-and-drop, priorities, due dates, comments
- [x] Documents: markdown editor/preview, filter, full-text search
- [x] Calendar: month grid, events
- [x] Invoices: line items, tax, totals, status workflow, print/PDF view
- [x] Channels: open-tag style channels/threads with agent teammates, message-to-task
- [x] Agents: OpenClaw console (config, status, sessions, message), teammates, activity
- [x] Terminal: PTY-backed xterm.js with tabs and streaming output

## Integrations
- [x] OpenClaw gateway bridge (sessions, messages, automations endpoints)
- [x] MCP server exposing Businex tools (`/api/mcp`, JSON-RPC 2.0)
- [x] Agent skill for OpenClaw (`integrations/openclaw/SKILL.md`)
- [x] open-tag bridge (daemon dispatch, thread replies) + native channels

## Roadmap
- [ ] Webhooks outbound, CSV import
- [ ] Email ingestion / calendar CalDAV sync
- [ ] Electron desktop wrapper around the web shell
