import { param } from "../context";
import { Hono } from "hono";
import { getDb, now, parseJson } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { bus } from "../events";
import { getOpenClawConfig, saveOpenClawConfig, OpenClawClient } from "../lib/openclaw";
import { getOpenTagConfig, OpenTagClient } from "../lib/opentag";

export const agentRoutes = new Hono<Env>();
agentRoutes.use("*", authenticate, requireWorkspace);

const agentRow = (r: any) => ({
  id: r.id, name: r.name, kind: r.kind, status: r.status,
  config: parseJson<Record<string, unknown>>(r.config, {}), createdAt: r.created_at, updatedAt: r.updated_at,
});

// Agent teammates -------------------------------------------------------------

agentRoutes.get("/agents", requireScope("agents:read", "viewer"), async (c) => {
  const a = auth(c);
  const rows = getDb().prepare("SELECT * FROM agents WHERE workspace_id = ? ORDER BY created_at").all(a.workspaceId) as any[];
  return c.json({ items: rows.map(agentRow) });
});

agentRoutes.post("/agents", requireScope("agents:write", "admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const name = String(body.name ?? "").trim();
  if (!name) throw badRequest("name is required");
  const kind = ["openclaw", "open-tag", "builtin"].includes(body.kind) ? body.kind : "openclaw";
  const id = newId("agt");
  const t = now();
  getDb().prepare(`
    INSERT INTO agents (id, workspace_id, name, kind, status, config, created_at, updated_at)
    VALUES (?, ?, ?, ?, 'idle', ?, ?, ?)
  `).run(id, a.workspaceId, name, kind, JSON.stringify(body.config ?? {}), t, t);
  audit(a.workspaceId, actor(c), "agent.create", "agent", id, { name, kind });
  return c.json(agentRow(getDb().prepare("SELECT * FROM agents WHERE id = ?").get(id)), 201);
});

agentRoutes.patch("/agents/:id", requireScope("agents:write", "admin"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM agents WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Agent not found");
  const body = await c.req.json().catch(() => ({}));
  const config = body.config && typeof body.config === "object" ? JSON.stringify(body.config) : existing.config;
  db.prepare("UPDATE agents SET name = ?, config = ?, status = ?, updated_at = ? WHERE id = ?")
    .run(body.name ? String(body.name) : existing.name, config, body.status ? String(body.status) : existing.status, now(), existing.id);
  audit(a.workspaceId, actor(c), "agent.update", "agent", existing.id);
  return c.json(agentRow(db.prepare("SELECT * FROM agents WHERE id = ?").get(existing.id)));
});

agentRoutes.delete("/agents/:id", requireScope("agents:write", "admin"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM agents WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  audit(a.workspaceId, actor(c), "agent.delete", "agent", param(c, "id"));
  return c.json({ ok: true });
});

// Agent activity --------------------------------------------------------------

agentRoutes.get("/agents/events", requireScope("agents:read", "viewer"), async (c) => {
  const a = auth(c);
  const limit = Math.min(Number(c.req.query("limit") ?? 50), 200);
  const rows = getDb().prepare("SELECT * FROM agent_events WHERE workspace_id = ? ORDER BY created_at DESC LIMIT ?")
    .all(a.workspaceId, limit) as any[];
  return c.json({ items: rows.map((r) => ({
    id: r.id, agentId: r.agent_id, kind: r.kind,
    payload: parseJson<Record<string, unknown> | null>(r.payload, null), createdAt: r.created_at,
  })) });
});

// OpenClaw bridge -------------------------------------------------------------

agentRoutes.get("/integrations/openclaw", requireScope("agents:read", "admin"), async (c) => {
  const cfg = getOpenClawConfig(auth(c).workspaceId);
  return c.json({
    configured: cfg.enabled,
    gatewayUrl: cfg.gatewayUrl,
    defaultAgent: cfg.defaultAgent,
    hasToken: Boolean(cfg.apiToken),
  });
});

agentRoutes.put("/integrations/openclaw", requireScope("agents:write", "admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  saveOpenClawConfig(a.workspaceId, {
    gatewayUrl: body.gatewayUrl ? String(body.gatewayUrl) : null,
    apiToken: body.apiToken ? String(body.apiToken) : undefined as any,
    defaultAgent: body.defaultAgent ? String(body.defaultAgent) : null,
  });
  audit(a.workspaceId, actor(c), "integration.update", "integration", "openclaw");
  return c.json({ ok: true });
});

agentRoutes.get("/integrations/openclaw/status", requireScope("agents:read", "admin"), async (c) => {
  const client = new OpenClawClient(getOpenClawConfig(auth(c).workspaceId));
  return c.json(await client.status());
});

agentRoutes.get("/integrations/openclaw/agents", requireScope("agents:read", "admin"), async (c) => {
  const client = new OpenClawClient(getOpenClawConfig(auth(c).workspaceId));
  return c.json(await client.agents());
});

agentRoutes.get("/integrations/openclaw/sessions", requireScope("agents:read", "admin"), async (c) => {
  const client = new OpenClawClient(getOpenClawConfig(auth(c).workspaceId));
  return c.json(await client.sessions());
});

agentRoutes.get("/integrations/openclaw/automations", requireScope("agents:read", "admin"), async (c) => {
  const client = new OpenClawClient(getOpenClawConfig(auth(c).workspaceId));
  return c.json(await client.automations());
});

agentRoutes.post("/integrations/openclaw/message", requireScope("agents:write", "admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const sessionKey = String(body.sessionKey ?? "");
  const message = String(body.message ?? "").trim();
  if (!sessionKey || !message) throw badRequest("sessionKey and message are required");
  const client = new OpenClawClient(getOpenClawConfig(a.workspaceId));
  const result = await client.sendMessage(sessionKey, message);
  audit(a.workspaceId, actor(c), "openclaw.message", "agent_session", sessionKey, { ok: result.ok });
  return c.json(result, result.ok ? 200 : 502);
});

/**
 * Dispatch an agent mention: records the event and, when the OpenClaw bridge is
 * configured, forwards the thread context to the gateway session. The reply is
 * posted back to the channel as an agent-authored message.
 */
bus.on("event", (event) => {
  if (event.type !== "channel.mention") return;
  const payload = event.payload as { channelId: string; messageId: string; mentions: string[]; body: string; threadId: string | null };
  const db = getDb();
  const t = now();
  for (const mention of payload.mentions) {
    const agent = db.prepare("SELECT * FROM agents WHERE workspace_id = ? AND (name = ? OR name = ?)")
      .get(event.workspaceId, mention, "@" + mention) as any;
    if (!agent) continue;
    db.prepare("INSERT INTO agent_events (id, workspace_id, agent_id, kind, payload, created_at) VALUES (?, ?, ?, 'mention', ?, ?)")
      .run(newId("aev"), event.workspaceId, agent.id, JSON.stringify(payload), t);

    // open-tag managed agents dispatch through the daemon; OpenClaw agents use the gateway.
    if (agent.kind === "open-tag") {
      const otCfg = getOpenTagConfig(event.workspaceId);
      if (otCfg.enabled) {
        const ot = new OpenTagClient(otCfg);
        ot.dispatch(otCfg.defaultAgent ?? agent.name, payload.body, {
          channelId: payload.channelId, messageId: payload.messageId, threadId: payload.threadId,
        }).then((result) => {
          if (!result.ok) return;
          const replyText = typeof result.data === "string" ? result.data : JSON.stringify(result.data).slice(0, 4000);
          const replyId = newId("msg");
          db.prepare(`
            INSERT INTO messages (id, workspace_id, channel_id, thread_id, author_type, author_id, body, meta, created_at)
            VALUES (?, ?, ?, ?, 'agent', ?, ?, ?, ?)
          `).run(replyId, event.workspaceId, payload.channelId, payload.threadId ?? payload.messageId,
            agent.id, replyText, JSON.stringify({ via: "open-tag", agent: agent.name }), new Date().toISOString());
          bus.publish("channel.message", event.workspaceId, { channelId: payload.channelId, message: { id: replyId } });
          db.prepare("UPDATE agents SET status = 'idle', updated_at = ? WHERE id = ?").run(new Date().toISOString(), agent.id);
        });
        db.prepare("UPDATE agents SET status = 'working', updated_at = ? WHERE id = ?").run(t, agent.id);
      }
      continue;
    }

    const cfg = getOpenClawConfig(event.workspaceId);
    if (cfg.enabled && cfg.defaultAgent) {
      const client = new OpenClawClient(cfg);
      const context = "You were mentioned in Businex channel message " + payload.messageId + ".\n\n" + payload.body;
      client.sendMessage(cfg.defaultAgent, context).then((result) => {
        if (!result.ok) return;
        const replyText = typeof result.data === "string" ? result.data : JSON.stringify(result.data).slice(0, 4000);
        const replyId = newId("msg");
        db.prepare(`
          INSERT INTO messages (id, workspace_id, channel_id, thread_id, author_type, author_id, body, meta, created_at)
          VALUES (?, ?, ?, ?, 'agent', ?, ?, ?, ?)
        `).run(replyId, event.workspaceId, payload.channelId, payload.threadId ?? payload.messageId,
          agent.id, replyText, JSON.stringify({ via: "openclaw", agent: agent.name }), new Date().toISOString());
        bus.publish("channel.message", event.workspaceId, {
          channelId: payload.channelId,
          message: { id: replyId, channelId: payload.channelId, threadId: payload.threadId ?? payload.messageId, authorType: "agent", authorId: agent.id, body: replyText, createdAt: new Date().toISOString() },
        });
        db.prepare("UPDATE agents SET status = 'idle', updated_at = ? WHERE id = ?").run(new Date().toISOString(), agent.id);
      });
      db.prepare("UPDATE agents SET status = 'working', updated_at = ? WHERE id = ?").run(t, agent.id);
    }
  }
});
