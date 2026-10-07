import { param } from "../context";
import { Hono } from "hono";
import { setCookie, deleteCookie, getCookie } from "hono/cookie";
import { z } from "zod";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { hashPassword, verifyPassword } from "../lib/password";
import { signToken, randomToken, hashToken, verifyToken } from "../lib/token";
import { badRequest, unauthorized, conflict, forbidden, notFound } from "../lib/errors";
import { authenticate, auth, requireWorkspace, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { config } from "../config";
import { registerSchema, loginSchema } from "@businex/shared";

export const authRoutes = new Hono<Env>();

const AVATAR_COLORS = ["#6366f1", "#8b5cf6", "#ec4899", "#f59e0b", "#10b981", "#06b6d4", "#ef4444", "#84cc16"];

function slugify(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 48) || "workspace";
}

function uniqueSlug(db: any, base: string): string {
  let slug = base, i = 2;
  while (db.prepare("SELECT 1 FROM workspaces WHERE slug = ?").get(slug)) slug = base + "-" + i++;
  return slug;
}

function publicUser(row: any) {
  return {
    id: row.id, email: row.email, name: row.name,
    avatarColor: row.avatar_color, createdAt: row.created_at, lastSeenAt: row.last_seen_at,
  };
}

function issueSession(db: any, userId: string, userAgent: string | undefined): string {
  const id = newId("ses");
  const token = signToken(id);
  db.prepare(`
    INSERT INTO sessions (id, user_id, token_hash, user_agent, created_at, expires_at)
    VALUES (?, ?, ?, ?, ?, ?)
  `).run(id, userId, hashToken(token), userAgent ?? null, now(), new Date(Date.now() + config.sessionTtlMs).toISOString());
  return token;
}

function sessionCookie() {
  return {
    httpOnly: true,
    secure: config.env === "production",
    sameSite: "Lax" as const,
    path: "/",
    maxAge: Math.floor(config.sessionTtlMs / 1000),
  };
}

authRoutes.post("/register", async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const parsed = registerSchema.safeParse(body);
  if (!parsed.success) throw badRequest("Invalid registration payload", parsed.error.flatten());
  const { email, name, password, workspaceName } = parsed.data;
  if (config.registrationEmails.length && !config.registrationEmails.includes(email.toLowerCase())) {
    throw forbidden("Registration is limited to invited email addresses");
  }

  const db = getDb();
  if (db.prepare("SELECT 1 FROM users WHERE email = ?").get(email)) throw conflict("An account with this email already exists");

  const passwordHash = await hashPassword(password);
  const userId = newId("usr");
  const orgId = newId("org");
  const wsId = newId("ws");
  const wsName = workspaceName?.trim() || name.split(" ")[0] + "'s Workspace";
  const t = now();

  const run = db.transaction(() => {
    db.prepare(`
      INSERT INTO users (id, email, name, password_hash, avatar_color, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run(userId, email, name, passwordHash, AVATAR_COLORS[Math.floor(Math.random() * AVATAR_COLORS.length)], t, t);
    db.prepare("INSERT INTO organizations (id, name, slug, created_at) VALUES (?, ?, ?, ?)")
      .run(orgId, wsName, uniqueSlug(db, slugify(wsName)), t);
    db.prepare(`
      INSERT INTO workspaces (id, org_id, name, slug, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `).run(wsId, orgId, wsName, uniqueSlug(db, slugify(wsName)), t, t);
    db.prepare("INSERT INTO memberships (id, user_id, workspace_id, role, created_at) VALUES (?, ?, ?, 'owner', ?)")
      .run(newId("mem"), userId, wsId, t);
    // A clean workspace: the general channel is the only seed data.
    const channelId = newId("chn");
    db.prepare("INSERT INTO channels (id, workspace_id, kind, name, topic, created_at) VALUES (?, ?, 'channel', 'general', 'Company-wide chatter', ?)")
      .run(channelId, wsId, t);
    db.prepare("INSERT INTO channel_members (channel_id, user_id, role, joined_at) VALUES (?, ?, 'owner', ?)")
      .run(channelId, userId, t);
    db.prepare(`
      INSERT INTO messages (id, workspace_id, channel_id, author_type, author_id, body, created_at)
      VALUES (?, ?, ?, 'system', NULL, 'Welcome to Businex. Your workspace is ready — invite your team, connect your agents, and start running the business here.', ?)
    `).run(newId("msg"), wsId, channelId, t);
    db.prepare("INSERT INTO user_prefs (user_id, workspace_id, prefs, updated_at) VALUES (?, ?, '{}', ?)").run(userId, wsId, t);
  });

  run();

  const token = issueSession(db, userId, c.req.header("user-agent"));
  audit(wsId, { type: "user", id: userId }, "workspace.create", "workspace", wsId, { name: wsName });
  setCookie(c, "businex_session", token, sessionCookie());
  const user = publicUser(db.prepare("SELECT * FROM users WHERE id = ?").get(userId));
  return c.json({
    token, user,
    workspace: { id: wsId, orgId, name: wsName, slug: slugify(wsName), createdAt: t },
  }, 201);
});

authRoutes.post("/login", async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const parsed = loginSchema.safeParse(body);
  if (!parsed.success) throw badRequest("Invalid login payload", parsed.error.flatten());
  const db = getDb();
  const row = db.prepare("SELECT * FROM users WHERE email = ?").get(parsed.data.email) as any;
  if (!row || !(await verifyPassword(parsed.data.password, row.password_hash))) {
    throw unauthorized("Invalid email or password");
  }
  db.prepare("UPDATE users SET last_seen_at = ? WHERE id = ?").run(now(), row.id);
  const token = issueSession(db, row.id, c.req.header("user-agent"));
  setCookie(c, "businex_session", token, sessionCookie());
  return c.json({ token, user: publicUser(row) });
});

authRoutes.post("/logout", authenticate, async (c) => {
  const a = auth(c);
  if (a.sessionId) getDb().prepare("DELETE FROM sessions WHERE id = ?").run(a.sessionId);
  deleteCookie(c, "businex_session");
  return c.json({ ok: true });
});

authRoutes.get("/me", authenticate, async (c) => {
  const a = auth(c);
  const db = getDb();
  const user = db.prepare("SELECT * FROM users WHERE id = ?").get(a.userId) as any;
  if (!user) throw unauthorized();
  const workspaces = db.prepare(`
    SELECT w.id, w.org_id, w.name, w.slug, w.created_at, m.role
    FROM memberships m JOIN workspaces w ON w.id = m.workspace_id
    WHERE m.user_id = ? ORDER BY m.created_at
  `).all(a.userId) as any[];
  return c.json({
    user: publicUser(user),
    workspaces: workspaces.map((w) => ({
      id: w.id, orgId: w.org_id, name: w.name, slug: w.slug, createdAt: w.created_at, role: w.role,
    })),
  });
});

// Invitations ---------------------------------------------------------------

authRoutes.post("/accept-invite", authenticate, async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const token = String(body.token ?? "");
  if (!token) throw badRequest("token is required");
  const db = getDb();
  const invite = db.prepare("SELECT * FROM invitations WHERE token = ?").get(token) as any;
  if (!invite) throw notFound("Invitation not found");
  if (invite.accepted_at) throw conflict("Invitation already accepted");
  if (a.userId !== (db.prepare("SELECT id FROM users WHERE email = ?").get(invite.email) as any)?.id) {
    throw forbidden("Invitation is for a different email address");
  }
  db.prepare("INSERT OR IGNORE INTO memberships (id, user_id, workspace_id, role, created_at) VALUES (?, ?, ?, ?, ?)")
    .run(newId("mem"), a.userId, invite.workspace_id, invite.role, now());
  db.prepare("UPDATE invitations SET accepted_at = ? WHERE id = ?").run(now(), invite.id);
  audit(invite.workspace_id, { type: "user", id: a.userId }, "invitation.accept", "invitation", invite.id);
  return c.json({ ok: true, workspaceId: invite.workspace_id });
});

// Passwords ------------------------------------------------------------------

authRoutes.post("/change-password", authenticate, async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const current = String(body.currentPassword ?? "");
  const next = String(body.newPassword ?? "");
  if (next.length < 8) throw badRequest("new password must be at least 8 characters");
  const db = getDb();
  const user = db.prepare("SELECT * FROM users WHERE id = ?").get(a.userId) as any;
  if (!user || !(await verifyPassword(current, user.password_hash))) throw unauthorized("Current password is incorrect");
  db.prepare("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?").run(await hashPassword(next), now(), user.id);
  if (a.sessionId) db.prepare("DELETE FROM sessions WHERE user_id = ? AND id != ?").run(user.id, a.sessionId);
  return c.json({ ok: true });
});

authRoutes.post("/forgot-password", async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const email = String(body.email ?? "").trim().toLowerCase();
  // Password recovery requires a verified mail delivery channel. Until configured,
  // return the same response for every address and never expose a reset credential.
  return c.json({ ok: true });
});

authRoutes.post("/reset-password", async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const token = String(body.token ?? "");
  const next = String(body.newPassword ?? "");
  if (next.length < 8) throw badRequest("new password must be at least 8 characters");
  const db = getDb();
  const row = db.prepare("SELECT * FROM password_resets WHERE token_hash = ?").get(hashToken(token)) as any;
  if (!row || row.used_at || new Date(row.expires_at).getTime() < Date.now()) throw unauthorized("Reset token is invalid or expired");
  db.prepare("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?").run(await hashPassword(next), now(), row.user_id);
  db.prepare("UPDATE password_resets SET used_at = ? WHERE id = ?").run(now(), row.id);
  db.prepare("DELETE FROM sessions WHERE user_id = ?").run(row.user_id);
  return c.json({ ok: true });
});

// API keys ------------------------------------------------------------------

authRoutes.post("/api-keys", authenticate, requireWorkspace, async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const name = String(body.name ?? "").trim();
  const scopes: string[] = Array.isArray(body.scopes) ? body.scopes.map(String) : ["*"];
  if (!name) throw badRequest("name is required");

  const id = newId("key");
  const secret = randomToken(24);
  const token = config.tokenPrefix + secret;
  getDb().prepare(`
    INSERT INTO api_keys (id, workspace_id, name, prefix, key_hash, scopes, created_by, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, name, config.tokenPrefix + secret.slice(0, 6), hashToken(token), JSON.stringify(scopes), a.userId, now());
  audit(a.workspaceId, actor(c), "apikey.create", "api_key", id, { name, scopes });
  // The full key is returned exactly once.
  return c.json({ id, name, scopes, token }, 201);
});

authRoutes.get("/api-keys", authenticate, requireWorkspace, async (c) => {
  const a = auth(c);
  const rows = getDb().prepare(`
    SELECT id, name, prefix, scopes, created_by, created_at, last_used_at, revoked_at
    FROM api_keys WHERE workspace_id = ? ORDER BY created_at DESC
  `).all(a.workspaceId) as any[];
  return c.json({ items: rows.map((r) => ({
    id: r.id, name: r.name, prefix: r.prefix,
    scopes: JSON.parse(r.scopes || "[]"), createdBy: r.created_by,
    createdAt: r.created_at, lastUsedAt: r.last_used_at, revokedAt: r.revoked_at,
  })) });
});

authRoutes.delete("/api-keys/:id", authenticate, requireWorkspace, async (c) => {
  const a = auth(c);
  getDb().prepare("UPDATE api_keys SET revoked_at = ? WHERE id = ? AND workspace_id = ?").run(now(), param(c, "id"), a.workspaceId);
  audit(a.workspaceId, actor(c), "apikey.revoke", "api_key", param(c, "id"));
  return c.json({ ok: true });
});
