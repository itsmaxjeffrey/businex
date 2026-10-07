import { Hono } from "hono";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { getOpenTagConfig, saveOpenTagConfig, OpenTagClient } from "../lib/opentag";
import { audit } from "../lib/audit";

export const openTagRoutes = new Hono<Env>();
openTagRoutes.use("*", authenticate, requireWorkspace);

openTagRoutes.get("/integrations/open-tag", requireScope("agents:read", "admin"), async (c) => {
  const cfg = getOpenTagConfig(auth(c).workspaceId);
  return c.json({
    configured: cfg.enabled,
    daemonUrl: cfg.daemonUrl,
    defaultAgent: cfg.defaultAgent,
    hasToken: Boolean(cfg.apiToken),
  });
});

openTagRoutes.put("/integrations/open-tag", requireScope("agents:write", "admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  saveOpenTagConfig(a.workspaceId, {
    daemonUrl: body.daemonUrl ? String(body.daemonUrl) : null,
    apiToken: body.apiToken ? String(body.apiToken) : (undefined as any),
    defaultAgent: body.defaultAgent ? String(body.defaultAgent) : null,
  });
  audit(a.workspaceId, actor(c), "integration.update", "integration", "open-tag");
  return c.json({ ok: true });
});

openTagRoutes.get("/integrations/open-tag/status", requireScope("agents:read", "admin"), async (c) => {
  const client = new OpenTagClient(getOpenTagConfig(auth(c).workspaceId));
  return c.json(await client.status());
});

openTagRoutes.get("/integrations/open-tag/agents", requireScope("agents:read", "admin"), async (c) => {
  const client = new OpenTagClient(getOpenTagConfig(auth(c).workspaceId));
  return c.json(await client.agents());
});
