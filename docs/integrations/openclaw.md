# OpenClaw integration

## What it gives you

1. **Agent console inside Businex** — see agents and sessions, send a message, read the reply,
   without leaving your business workspace.
2. **Agents operating business records** — via the MCP server at `/mcp` or scoped REST API keys,
   an OpenClaw agent can create/update CRM records, tasks, documents, invoices and post channel
   messages. Every call is audited.
3. **Agent teammates in channels** — mention `@agent` in a channel; Businex forwards the thread
   context to the OpenClaw session and posts the reply back to the thread.

## Configuration

Settings → Integrations → OpenClaw:

| Field | Meaning |
| --- | --- |
| Gateway URL | e.g. `http://127.0.0.1:18789` |
| API token | token used for gateway calls (stored encrypted-at-rest style, never logged) |
| Default agent | agent id used for channel mentions |

## API key scopes

API keys created in Settings → API keys are scoped per module:

```text
crm:read crm:write  projects:read projects:write  documents:read documents:write
invoices:read invoices:write  channels:read channels:write  analytics:read
```

Send them as `Authorization: Bearer bnx_<key>`.

## MCP tools

The MCP server exposes tools like `businex.search`, `businex.task.create`, `businex.contact.create`,
`businex.invoice.create`, `businex.document.write`, `businex.channel.post` — each bound to the
workspace of the API key.
