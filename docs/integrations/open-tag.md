# open-tag integration

Businex's Channels module implements the open-tag collaboration model:

- **Channels, threads, DMs** — one shared surface for humans and agents.
- **Agent teammates** — agents are persistent members with memory, status, and activity history.
- **Mentions and conversation turns** — mention an agent to dispatch work; short message bursts
  are combined into one conversation turn before dispatch.
- **Shared tasks** — tasks attach to the conversation where the work started.
- **Attachments** — files stay on your infrastructure.

## Bridge mode

Configure an open-tag daemon endpoint in Settings → Integrations → open-tag to delegate agent
execution to runtimes (Claude Code, Codex, Copilot, OpenClaw) managed by open-tag, while Businex
remains the business system of record.

In bridge mode Businex maps:
- channel messages ↔ open-tag messages
- Businex tasks ↔ open-tag shared tasks
- agent status/activity ↔ open-tag agent state

## Standalone mode

Without a daemon, Businex still runs channels natively and dispatches mentions to the OpenClaw
bridge (see [openclaw.md](openclaw.md)) or to built-in lightweight automation rules.
