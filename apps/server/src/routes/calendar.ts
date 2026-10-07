import { param } from "../context";
import { Hono } from "hono";
import { getDb, now, bool } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { indexEntity, removeFromIndex } from "../lib/search";
import { eventSchema } from "@businex/shared";

export const calendarRoutes = new Hono<Env>();
calendarRoutes.use("*", authenticate, requireWorkspace);

const eventRow = (r: any) => ({
  id: r.id, title: r.title, description: r.description, startsAt: r.starts_at, endsAt: r.ends_at,
  allDay: bool(r.all_day), location: r.location, color: r.color, createdBy: r.created_by,
  createdAt: r.created_at, updatedAt: r.updated_at,
});

calendarRoutes.get("/events", requireScope("calendar:read", "viewer"), async (c) => {
  const a = auth(c);
  const from = c.req.query("from");
  const to = c.req.query("to");
  const db = getDb();
  let rows: any[];
  if (from && to) {
    rows = db.prepare("SELECT * FROM events WHERE workspace_id = ? AND ends_at >= ? AND starts_at <= ? ORDER BY starts_at")
      .all(a.workspaceId, from, to) as any[];
  } else {
    rows = db.prepare("SELECT * FROM events WHERE workspace_id = ? ORDER BY starts_at LIMIT 200").all(a.workspaceId) as any[];
  }
  return c.json({ items: rows.map(eventRow) });
});

calendarRoutes.post("/events", requireScope("calendar:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = eventSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid event", parsed.error.flatten());
  const d = parsed.data;
  const id = newId("evt");
  const t = now();
  getDb().prepare(`
    INSERT INTO events (id, workspace_id, title, description, starts_at, ends_at, all_day, location, color, created_by, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.title, d.description ?? null, d.startsAt, d.endsAt, d.allDay ? 1 : 0,
    d.location ?? null, d.color ?? "#6366f1", a.userId, t, t);
  indexEntity(a.workspaceId, "event", id, d.title, d.description ?? "");
  audit(a.workspaceId, actor(c), "event.create", "event", id, { title: d.title });
  return c.json(eventRow(getDb().prepare("SELECT * FROM events WHERE id = ?").get(id)), 201);
});

calendarRoutes.patch("/events/:id", requireScope("calendar:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM events WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Event not found");
  const parsed = eventSchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid event", parsed.error.flatten());
  const merged = { ...existing };
  const map: Record<string, string> = {
    title: "title", description: "description", startsAt: "starts_at", endsAt: "ends_at",
    location: "location", color: "color",
  };
  for (const [k, col] of Object.entries(map)) if (k in parsed.data) merged[col] = (parsed.data as any)[k] ?? null;
  if ("allDay" in parsed.data) merged.all_day = parsed.data.allDay ? 1 : 0;
  db.prepare("UPDATE events SET title=?, description=?, starts_at=?, ends_at=?, all_day=?, location=?, color=?, updated_at=? WHERE id=?")
    .run(merged.title, merged.description, merged.starts_at, merged.ends_at, merged.all_day, merged.location, merged.color, now(), existing.id);
  indexEntity(a.workspaceId, "event", existing.id, merged.title, merged.description ?? "");
  audit(a.workspaceId, actor(c), "event.update", "event", existing.id);
  return c.json(eventRow(db.prepare("SELECT * FROM events WHERE id = ?").get(existing.id)));
});

calendarRoutes.delete("/events/:id", requireScope("calendar:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM events WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("event", param(c, "id"));
  audit(a.workspaceId, actor(c), "event.delete", "event", param(c, "id"));
  return c.json({ ok: true });
});
