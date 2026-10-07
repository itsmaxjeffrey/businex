import { param } from "../context";
import { Hono } from "hono";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound, conflict, forbidden } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireRole, actor, type Env } from "../context";
import { audit } from "../lib/audit";

export const workspaceRoutes = new Hono<Env>();

workspaceRoutes.use("*", authenticate, requireWorkspace);

workspaceRoutes.get("/", async (c) => {
  const a = auth(c);
  const db = getDb();
  const ws = db.prepare("SELECT * FROM workspaces WHERE id = ?").get(a.workspaceId) as any;
  if (!ws) throw notFound("Workspace not found");
  return c.json({
    workspace: { id: ws.id, orgId: ws.org_id, name: ws.name, slug: ws.slug, createdAt: ws.created_at, settings: JSON.parse(ws.settings || "{}") },
    role: a.role,
  });
});

workspaceRoutes.patch("/", requireRole("admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const name = typeof body.name === "string" ? body.name.trim() : null;
  const settings = body.settings && typeof body.settings === "object" ? body.settings : null;
  const db = getDb();
  if (name) db.prepare("UPDATE workspaces SET name = ?, updated_at = ? WHERE id = ?").run(name, now(), a.workspaceId);
  if (settings) db.prepare("UPDATE workspaces SET settings = ?, updated_at = ? WHERE id = ?").run(JSON.stringify(settings), now(), a.workspaceId);
  audit(a.workspaceId, actor(c), "workspace.update", "workspace", a.workspaceId, { name, settings: !!settings });
  return c.json({ ok: true });
});

workspaceRoutes.get("/members", async (c) => {
  const a = auth(c);
  const rows = getDb().prepare(`
    SELECT m.id, m.role, m.created_at, u.id AS user_id, u.name, u.email, u.avatar_color, u.last_seen_at
    FROM memberships m JOIN users u ON u.id = m.user_id
    WHERE m.workspace_id = ? ORDER BY m.created_at
  `).all(a.workspaceId) as any[];
  return c.json({ items: rows.map((r) => ({
    id: r.id, role: r.role, createdAt: r.created_at,
    user: { id: r.user_id, name: r.name, email: r.email, avatarColor: r.avatar_color, lastSeenAt: r.last_seen_at },
  })) });
});

workspaceRoutes.post("/members", requireRole("admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const email = String(body.email ?? "").trim().toLowerCase();
  const name = String(body.name ?? "").trim();
  const role = String(body.role ?? "member");
  if (!email) throw badRequest("email is required");
  const db = getDb();

  let user = db.prepare("SELECT * FROM users WHERE email = ?").get(email) as any;
  if (!user) {
    if (!name) throw badRequest("name is required for new users");
    const id = newId("usr");
    const t = now();
    // Invite flow: user is created without a password; they set it on first login via invite token.
    db.prepare(`
      INSERT INTO users (id, email, name, password_hash, avatar_color, created_at, updated_at)
      VALUES (?, ?, ?, '', '#6366f1', ?, ?)
    `).run(id, email, name, t, t);
    user = db.prepare("SELECT * FROM users WHERE id = ?").get(id);
  }
  const existing = db.prepare("SELECT 1 FROM memberships WHERE user_id = ? AND workspace_id = ?").get(user.id, a.workspaceId);
  if (existing) throw conflict("Already a member of this workspace");

  db.prepare("INSERT INTO memberships (id, user_id, workspace_id, role, created_at) VALUES (?, ?, ?, ?, ?)")
    .run(newId("mem"), user.id, a.workspaceId, role, now());
  audit(a.workspaceId, actor(c), "member.add", "membership", user.id, { email, role });
  return c.json({ ok: true, userId: user.id }, 201);
});

workspaceRoutes.patch("/members/:userId", requireRole("admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const role = String(body.role ?? "");
  if (!["owner", "admin", "member", "agent", "viewer"].includes(role)) throw badRequest("invalid role");
  const res = getDb().prepare("UPDATE memberships SET role = ? WHERE user_id = ? AND workspace_id = ?")
    .run(role, param(c, "userId"), a.workspaceId);
  if (res.changes === 0) throw notFound("Membership not found");
  audit(a.workspaceId, actor(c), "member.role", "membership", param(c, "userId"), { role });
  return c.json({ ok: true });
});

workspaceRoutes.delete("/members/:userId", requireRole("admin"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const ownerCount = db.prepare("SELECT COUNT(*) AS n FROM memberships WHERE workspace_id = ? AND role = 'owner'").get(a.workspaceId) as any;
  const target = db.prepare("SELECT role FROM memberships WHERE user_id = ? AND workspace_id = ?").get(param(c, "userId"), a.workspaceId) as any;
  if (!target) throw notFound("Membership not found");
  if (target.role === "owner" && ownerCount.n <= 1) throw forbidden("Cannot remove the last owner");
  db.prepare("DELETE FROM memberships WHERE user_id = ? AND workspace_id = ?").run(param(c, "userId"), a.workspaceId);
  audit(a.workspaceId, actor(c), "member.remove", "membership", param(c, "userId"));
  return c.json({ ok: true });
});

workspaceRoutes.get("/audit", requireRole("admin"), async (c) => {
  const a = auth(c);
  const limit = Math.min(Number(c.req.query("limit") ?? 100), 500);
  const rows = getDb().prepare(`
    SELECT * FROM audit_log WHERE workspace_id = ? ORDER BY created_at DESC LIMIT ?
  `).all(a.workspaceId, limit) as any[];
  return c.json({ items: rows.map((r) => ({
    id: r.id, actorType: r.actor_type, actorId: r.actor_id, action: r.action,
    entityType: r.entity_type, entityId: r.entity_id,
    meta: r.meta ? JSON.parse(r.meta) : null, createdAt: r.created_at,
  })) });
});

workspaceRoutes.get("/prefs", async (c) => {
  const a = auth(c);
  const row = getDb().prepare("SELECT prefs FROM user_prefs WHERE user_id = ? AND workspace_id = ?").get(a.userId, a.workspaceId) as any;
  return c.json({ prefs: row ? JSON.parse(row.prefs) : {} });
});

// Teams ------------------------------------------------------------------------

workspaceRoutes.get("/teams", async (c) => {
  const a = auth(c);
  const db = getDb();
  const teams = db.prepare("SELECT * FROM teams WHERE workspace_id = ? ORDER BY created_at").all(a.workspaceId) as any[];
  const links = db.prepare("SELECT team_id, user_id FROM team_members").all() as any[];
  return c.json({
    items: teams.map((t) => ({
      id: t.id,
      name: t.name,
      createdAt: t.created_at,
      memberIds: links.filter((l) => l.team_id === t.id).map((l) => l.user_id),
    })),
  });
});

workspaceRoutes.post("/teams", requireRole("admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const name = String(body.name ?? "").trim();
  if (!name) throw badRequest("name is required");
  const db = getDb();
  const id = newId("tem");
  db.prepare("INSERT INTO teams (id, workspace_id, name, created_at) VALUES (?, ?, ?, ?)").run(id, a.workspaceId, name, now());
  const memberIds: string[] = Array.isArray(body.memberIds) ? body.memberIds.map(String) : [];
  for (const userId of memberIds) {
    db.prepare("INSERT OR IGNORE INTO team_members (team_id, user_id) VALUES (?, ?)").run(id, userId);
  }
  audit(a.workspaceId, actor(c), "team.create", "team", id, { name });
  return c.json({ id, name, memberIds }, 201);
});

workspaceRoutes.post("/teams/:id/members", requireRole("admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const userId = String(body.userId ?? "");
  if (!userId) throw badRequest("userId is required");
  const db = getDb();
  const team = db.prepare("SELECT 1 FROM teams WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId);
  if (!team) throw notFound("Team not found");
  db.prepare("INSERT OR IGNORE INTO team_members (team_id, user_id) VALUES (?, ?)").run(param(c, "id"), userId);
  audit(a.workspaceId, actor(c), "team.member_add", "team", param(c, "id"), { userId });
  return c.json({ ok: true });
});

workspaceRoutes.delete("/teams/:id/members/:userId", requireRole("admin"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM team_members WHERE team_id = ? AND user_id = ?").run(param(c, "id"), param(c, "userId"));
  audit(a.workspaceId, actor(c), "team.member_remove", "team", param(c, "id"), { userId: param(c, "userId") });
  return c.json({ ok: true });
});

workspaceRoutes.delete("/teams/:id", requireRole("admin"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM teams WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  audit(a.workspaceId, actor(c), "team.delete", "team", param(c, "id"));
  return c.json({ ok: true });
});

// Invitations -----------------------------------------------------------------
// Local-first: the invite token is returned in the response. Put a mailer in
// front of this endpoint for production delivery.

workspaceRoutes.get("/invitations", requireRole("admin"), async (c) => {
  const a = auth(c);
  const rows = getDb().prepare("SELECT * FROM invitations WHERE workspace_id = ? ORDER BY created_at DESC").all(a.workspaceId) as any[];
  return c.json({
    items: rows.map((r) => ({
      id: r.id, email: r.email, role: r.role, token: r.token,
      createdBy: r.created_by, createdAt: r.created_at, acceptedAt: r.accepted_at,
    })),
  });
});

workspaceRoutes.post("/invitations", requireRole("admin"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const email = String(body.email ?? "").trim().toLowerCase();
  const role = String(body.role ?? "member");
  if (!email) throw badRequest("email is required");
  if (!["owner", "admin", "member", "agent", "viewer"].includes(role)) throw badRequest("invalid role");
  const id = newId("inv");
  const token = newId("tok") + Math.random().toString(36).slice(2, 12);
  getDb().prepare(`
    INSERT INTO invitations (id, workspace_id, email, role, token, created_by, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, email, role, token, a.userId, now());
  audit(a.workspaceId, actor(c), "invitation.create", "invitation", id, { email, role });
  return c.json({ id, email, role, token }, 201);
});

workspaceRoutes.delete("/invitations/:id", requireRole("admin"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM invitations WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  return c.json({ ok: true });
});

workspaceRoutes.put("/prefs", async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  getDb().prepare(`
    INSERT INTO user_prefs (user_id, workspace_id, prefs, updated_at) VALUES (?, ?, ?, ?)
    ON CONFLICT (user_id, workspace_id) DO UPDATE SET prefs = excluded.prefs, updated_at = excluded.updated_at
  `).run(a.userId, a.workspaceId, JSON.stringify(body ?? {}), now());
  return c.json({ ok: true });
});
