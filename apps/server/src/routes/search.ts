import { param } from "../context";
import { Hono } from "hono";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { badRequest } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { search } from "../lib/search";

export const searchRoutes = new Hono<Env>();
searchRoutes.use("*", authenticate, requireWorkspace);

searchRoutes.get("/search", requireScope("search:read", "viewer"), async (c) => {
  const a = auth(c);
  const q = c.req.query("q") ?? "";
  const limit = Math.min(Number(c.req.query("limit") ?? 20), 100);
  return c.json({ items: search(a.workspaceId, q, limit) });
});

searchRoutes.get("/tags", requireScope("search:read", "viewer"), async (c) => {
  const a = auth(c);
  const rows = getDb().prepare("SELECT * FROM tags WHERE workspace_id = ? ORDER BY name").all(a.workspaceId) as any[];
  return c.json({ items: rows.map((r) => ({ id: r.id, name: r.name, color: r.color, createdAt: r.created_at })) });
});

searchRoutes.post("/tags", requireScope("search:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const name = String(body.name ?? "").trim().toLowerCase();
  if (!name) throw badRequest("name is required");
  const db = getDb();
  const existing = db.prepare("SELECT * FROM tags WHERE workspace_id = ? AND name = ?").get(a.workspaceId, name) as any;
  if (existing) return c.json({ id: existing.id, name: existing.name, color: existing.color, createdAt: existing.created_at });
  const id = newId("tag");
  db.prepare("INSERT INTO tags (id, workspace_id, name, color, created_at) VALUES (?, ?, ?, ?, ?)")
    .run(id, a.workspaceId, name, String(body.color ?? "#64748b"), now());
  return c.json({ id, name }, 201);
});

searchRoutes.post("/tags/assign", requireScope("search:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const { entityType, entityId, tagId } = body as { entityType?: string; entityId?: string; tagId?: string };
  if (!entityType || !entityId || !tagId) throw badRequest("entityType, entityId and tagId are required");
  const db = getDb();
  db.prepare("INSERT OR IGNORE INTO entity_tags (workspace_id, entity_type, entity_id, tag_id) VALUES (?, ?, ?, ?)")
    .run(a.workspaceId, entityType, entityId, tagId);
  audit(a.workspaceId, actor(c), "tag.assign", entityType, entityId, { tagId });
  return c.json({ ok: true });
});

searchRoutes.delete("/tags/assign", requireScope("search:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const { entityType, entityId, tagId } = body as { entityType?: string; entityId?: string; tagId?: string };
  if (!entityType || !entityId || !tagId) throw badRequest("entityType, entityId and tagId are required");
  getDb().prepare("DELETE FROM entity_tags WHERE workspace_id = ? AND entity_type = ? AND entity_id = ? AND tag_id = ?")
    .run(a.workspaceId, entityType, entityId, tagId);
  return c.json({ ok: true });
});

searchRoutes.get("/tags/:id/entities", requireScope("search:read", "viewer"), async (c) => {
  const a = auth(c);
  const rows = getDb().prepare("SELECT entity_type, entity_id FROM entity_tags WHERE workspace_id = ? AND tag_id = ?")
    .all(a.workspaceId, param(c, "id")) as any[];
  return c.json({ items: rows.map((r) => ({ entityType: r.entity_type, entityId: r.entity_id })) });
});
