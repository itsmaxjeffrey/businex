# open-tag bridge

Businex ships native open-tag style collaboration (channels, threads, DMs,
shared tasks, agent mentions) and can additionally bridge agent execution to an
open-tag daemon.

## Standalone mode (default)

Channels run natively inside Businex. Mentions of `@agent` names are dispatched
through the OpenClaw bridge (see ../openclaw/SKILL.md) or recorded as agent
events. No daemon required.

## Bridge mode

1. Run an open-tag daemon on your network.
2. In Businex: Settings → Agents → open-tag bridge → set the daemon URL and token.
3. Create agent teammates with kind `open-tag`.

Dispatch flow:

```text
user mentions @agent in a Businex channel
  → Businex records the mention (agent_events)
  → open-tag daemon runs the agent runtime (Claude Code, Codex, Copilot, ...)
  → daemon posts the reply
  → Businex writes it back into the originating thread as an agent message
```

Businex remains the business system of record: tasks, contacts, invoices and
documents always live in Businex regardless of which runtime produced the work.
