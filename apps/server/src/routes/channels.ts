import { param } from "../context";
import { Hono } from "hono";
import { getDb, now, parseJson } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound, forbidden } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { indexEntity } from "../lib/search";
import { channelSchema, messageSchema } from "@businex/shared";
import { bus } from "../events";

export const channelRoutes = new Hono<Env>();
channelRoutes.use("*", authenticate, requireWorkspace);

/** Message-level operations live at /api/messages/* */
export const messageRoutes = new Hono<Env>();
messageRoutes.use("*", authenticate, requireWorkspace);

const channelRow = (r: any, extra: Record<string, unknown> = {}) => ({
  id: r.id, kind: r.kind, name: r.name, topic: r.topic, isPrivate: r.is_private === 1,
  createdBy: r.created_by, createdAt: r.created_at, ...extra,
});

const messageRow = (r: any) => ({
  id: r.id, channelId: r.channel_id, threadId: r.thread_id,
  authorType: r.author_type, authorId: r.author_id, body: r.body,
  meta: parseJson<Record<string, unknown> | null>(r.meta, null), createdAt: r.created_at,
});

function canAccessChannel(db: any, workspaceId: string, userId: string | null, channelId: string): boolean {
  const channel = db.prepare("SELECT * FROM channels WHERE id = ? AND workspace_id = ?").get(channelId, workspaceId) as any;
  if (!channel) return false;
  if (channel.is_private !== 1) return true;
  if (!userId) return true; // API keys operate with workspace-level access
  return !!db.prepare("SELECT 1 FROM channel_members WHERE channel_id = ? AND user_id = ?").get(channelId, userId);
}

channelRoutes.get("/", requireScope("channels:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const rows = db.prepare(`
    SELECT ch.*, (SELECT COUNT(*) FROM messages m WHERE m.channel_id = ch.id) AS message_count
    FROM channels ch WHERE ch.workspace_id = ? ORDER BY ch.kind, ch.name
  `).all(a.workspaceId) as any[];
  return c.json({ items: rows.map((r) => channelRow(r, { messageCount: r.message_count })) });
});

channelRoutes.post("/", requireScope("channels:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = channelSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid channel", parsed.error.flatten());
  const d = parsed.data;
  const db = getDb();
  const id = newId("chn");
  const t = now();
  db.prepare(`
    INSERT INTO channels (id, workspace_id, kind, name, topic, is_private, created_by, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.kind, d.name, d.topic ?? null, d.isPrivate ? 1 : 0, a.userId, t);
  db.prepare("INSERT INTO channel_members (channel_id, user_id, role, joined_at) VALUES (?, ?, 'owner', ?)").run(id, a.userId, t);
  audit(a.workspaceId, actor(c), "channel.create", "channel", id, { name: d.name });
  bus.publish("channel.created", a.workspaceId, { channelId: id });
  return c.json(channelRow(db.prepare("SELECT * FROM channels WHERE id = ?").get(id)), 201);
});

channelRoutes.post("/:id/join", requireScope("channels:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  if (!db.prepare("SELECT 1 FROM channels WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId)) throw notFound("Channel not found");
  db.prepare("INSERT OR IGNORE INTO channel_members (channel_id, user_id, role, joined_at) VALUES (?, ?, 'member', ?)")
    .run(param(c, "id"), a.userId, now());
  return c.json({ ok: true });
});

channelRoutes.get("/:id/messages", requireScope("channels:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  if (!canAccessChannel(db, a.workspaceId, a.userId, param(c, "id"))) throw forbidden("Not a member of this channel");
  const limit = Math.min(Number(c.req.query("limit") ?? 50), 200);
  const before = c.req.query("before");
  const threadId = c.req.query("threadId");
  let rows: any[];
  if (threadId) {
    rows = db.prepare(`
      SELECT * FROM messages WHERE channel_id = ? AND (id = ? OR thread_id = ?) ORDER BY created_at LIMIT ?
    `).all(param(c, "id"), threadId, threadId, limit);
  } else if (before) {
    rows = db.prepare(`
      SELECT * FROM messages WHERE channel_id = ? AND thread_id IS NULL AND created_at < ? ORDER BY created_at DESC LIMIT ?
    `).all(param(c, "id"), before, limit);
  } else {
    rows = db.prepare(`
      SELECT * FROM messages WHERE channel_id = ? AND thread_id IS NULL ORDER BY created_at DESC LIMIT ?
    `).all(param(c, "id"), limit);
  }
  return c.json({ items: rows.reverse().map(messageRow) });
});

channelRoutes.post("/:id/messages", requireScope("channels:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  if (!canAccessChannel(db, a.workspaceId, a.userId, param(c, "id"))) throw forbidden("Not a member of this channel");
  const parsed = messageSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid message", parsed.error.flatten());
  const id = newId("msg");
  db.prepare(`
    INSERT INTO messages (id, workspace_id, channel_id, thread_id, author_type, author_id, body, meta, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, param(c, "id"), parsed.data.threadId ?? null,
    a.actorType === "agent" ? "agent" : "user", a.userId ?? a.apiKeyId, parsed.data.body, null, now());
  const row = db.prepare("SELECT * FROM messages WHERE id = ?").get(id) as any;
  bus.publish("channel.message", a.workspaceId, { channelId: param(c, "id"), message: messageRow(row) });
  // Mentions dispatch to the agents module (registered by the agents route).
  const mentionRegex = /@([a-zA-Z0-9_-]+)/g;
  const mentions = [...parsed.data.body.matchAll(mentionRegex)].map((m) => m[1]);
  if (mentions.length) bus.publish("channel.mention", a.workspaceId, { channelId: param(c, "id"), messageId: id, mentions, body: parsed.data.body, threadId: parsed.data.threadId ?? null });
  return c.json(messageRow(row), 201);
});

/** Convert a message into a tracked task (open-tag shared-task model). */
messageRoutes.post("/:id/task", requireScope("channels:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const message = db.prepare("SELECT * FROM messages WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!message) throw notFound("Message not found");
  const body = await c.req.json().catch(() => ({}));
  const title = String(body.title ?? message.body).slice(0, 240);
  const id = newId("tsk");
  const t = now();
  const maxPos = (db.prepare("SELECT COALESCE(MAX(position), 0) AS p FROM tasks WHERE workspace_id = ?").get(a.workspaceId) as any).p;
  db.prepare(`
    INSERT INTO tasks (id, workspace_id, project_id, title, description, status, priority, position, assignee_id, message_id, created_by, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, 'todo', 'medium', ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, body.projectId ?? null, title, message.body, maxPos + 1, body.assigneeId ?? null, message.id, a.userId, t, t);
  indexEntity(a.workspaceId, "task", id, title, message.body);
  audit(a.workspaceId, actor(c), "task.create_from_message", "task", id, { messageId: message.id });
  bus.publish("task.created", a.workspaceId, { taskId: id, fromMessageId: message.id });
  return c.json({ id, messageId: message.id }, 201);
});

channelRoutes.get("/:id/threads", requireScope("channels:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const rows = db.prepare(`
    SELECT t.id, t.body, t.author_type, t.author_id, t.created_at,
           (SELECT COUNT(*) FROM messages r WHERE r.thread_id = t.id) AS replies
    FROM messages t
    WHERE t.channel_id = ? AND t.id IN (SELECT DISTINCT thread_id FROM messages WHERE thread_id IS NOT NULL AND channel_id = ?)
    ORDER BY t.created_at DESC LIMIT 50
  `).all(param(c, "id"), param(c, "id")) as any[];
  return c.json({ items: rows.map((r) => ({
    id: r.id, body: r.body, authorType: r.author_type, authorId: r.author_id,
    createdAt: r.created_at, replies: r.replies,
  })) });
});
