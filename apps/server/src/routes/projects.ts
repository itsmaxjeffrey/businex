import { param } from "../context";
import { Hono } from "hono";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { indexEntity, removeFromIndex } from "../lib/search";
import { projectSchema, taskSchema, TASK_STATUSES, type TaskStatus } from "@businex/shared";

export const projectRoutes = new Hono<Env>();
projectRoutes.use("*", authenticate, requireWorkspace);

const projectRow = (r: any) => ({
  id: r.id, name: r.name, key: r.key, description: r.description, status: r.status,
  color: r.color, dueDate: r.due_date, createdBy: r.created_by, createdAt: r.created_at, updatedAt: r.updated_at,
});
const taskRow = (r: any) => ({
  id: r.id, projectId: r.project_id, title: r.title, description: r.description, status: r.status,
  priority: r.priority, position: r.position, assigneeId: r.assignee_id, dueDate: r.due_date,
  messageId: r.message_id, createdBy: r.created_by, createdAt: r.created_at, updatedAt: r.updated_at,
});

function nextProjectKey(db: any, workspaceId: string, base: string): string {
  const key = base.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 8) || "PROJ";
  let candidate = key, i = 2;
  while (db.prepare("SELECT 1 FROM projects WHERE workspace_id = ? AND key = ?").get(workspaceId, candidate)) {
    candidate = key + i++;
  }
  return candidate;
}

// Projects --------------------------------------------------------------------

projectRoutes.get("/", requireScope("projects:read", "viewer"), async (c) => {
  const a = auth(c);
  const rows = getDb().prepare("SELECT * FROM projects WHERE workspace_id = ? ORDER BY created_at DESC").all(a.workspaceId) as any[];
  return c.json({ items: rows.map(projectRow) });
});

projectRoutes.post("/", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = projectSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid project", parsed.error.flatten());
  const d = parsed.data;
  const db = getDb();
  const id = newId("prj");
  const t = now();
  const key = nextProjectKey(db, a.workspaceId, d.key ?? d.name);
  db.prepare(`
    INSERT INTO projects (id, workspace_id, name, key, description, status, color, due_date, created_by, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.name, key, d.description ?? null, d.status, d.color ?? "#6366f1", d.dueDate ?? null, a.userId, t, t);
  indexEntity(a.workspaceId, "project", id, d.name, d.description ?? "");
  audit(a.workspaceId, actor(c), "project.create", "project", id, { name: d.name, key });
  return c.json(projectRow(db.prepare("SELECT * FROM projects WHERE id = ?").get(id)), 201);
});

projectRoutes.get("/:id", requireScope("projects:read", "viewer"), async (c) => {
  const row = getDb().prepare("SELECT * FROM projects WHERE id = ? AND workspace_id = ?").get(param(c, "id"), auth(c).workspaceId) as any;
  if (!row) throw notFound("Project not found");
  return c.json(projectRow(row));
});

projectRoutes.patch("/:id", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM projects WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Project not found");
  const parsed = projectSchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid project", parsed.error.flatten());
  const merged = { ...existing, ...Object.fromEntries(Object.entries(parsed.data).map(([k, v]) => [k, v ?? null])) };
  db.prepare("UPDATE projects SET name=?, description=?, status=?, color=?, due_date=?, updated_at=? WHERE id=?")
    .run(merged.name, merged.description, merged.status, merged.color, merged.due_date ?? merged.dueDate, now(), existing.id);
  indexEntity(a.workspaceId, "project", existing.id, merged.name, merged.description ?? "");
  audit(a.workspaceId, actor(c), "project.update", "project", existing.id);
  return c.json(projectRow(db.prepare("SELECT * FROM projects WHERE id = ?").get(existing.id)));
});

projectRoutes.delete("/:id", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM projects WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("project", param(c, "id"));
  audit(a.workspaceId, actor(c), "project.delete", "project", param(c, "id"));
  return c.json({ ok: true });
});

// Tasks -----------------------------------------------------------------------

projectRoutes.get("/tasks", requireScope("projects:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const filters: string[] = ["workspace_id = ?"];
  const args: any[] = [a.workspaceId];
  if (c.req.query("projectId")) { filters.push("project_id = ?"); args.push(c.req.query("projectId")); }
  if (c.req.query("status")) { filters.push("status = ?"); args.push(c.req.query("status")); }
  if (c.req.query("assigneeId")) { filters.push("assignee_id = ?"); args.push(c.req.query("assigneeId")); }
  const rows = db.prepare("SELECT * FROM tasks WHERE " + filters.join(" AND ") + " ORDER BY position, created_at").all(...args) as any[];
  return c.json({ items: rows.map(taskRow) });
});

projectRoutes.post("/tasks", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = taskSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid task", parsed.error.flatten());
  const d = parsed.data;
  const db = getDb();
  const id = newId("tsk");
  const t = now();
  const maxPos = (db.prepare("SELECT COALESCE(MAX(position), 0) AS p FROM tasks WHERE workspace_id = ? AND status = ?").get(a.workspaceId, d.status) as any).p;
  db.prepare(`
    INSERT INTO tasks (id, workspace_id, project_id, title, description, status, priority, position, assignee_id, due_date, created_by, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.projectId ?? null, d.title, d.description ?? null, d.status, d.priority,
    d.position ?? maxPos + 1, d.assigneeId ?? null, d.dueDate ?? null, a.userId, t, t);
  indexEntity(a.workspaceId, "task", id, d.title, d.description ?? "");
  audit(a.workspaceId, actor(c), "task.create", "task", id, { title: d.title, status: d.status });
  return c.json(taskRow(db.prepare("SELECT * FROM tasks WHERE id = ?").get(id)), 201);
});

projectRoutes.patch("/tasks/:id", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM tasks WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Task not found");
  const parsed = taskSchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid task", parsed.error.flatten());
  const map: Record<string, string> = {
    projectId: "project_id", title: "title", description: "description", status: "status",
    priority: "priority", position: "position", assigneeId: "assignee_id", dueDate: "due_date",
  };
  const merged = { ...existing };
  for (const [k, col] of Object.entries(map)) if (k in parsed.data) merged[col] = (parsed.data as any)[k] ?? null;
  if (merged.status && !TASK_STATUSES.includes(merged.status)) throw badRequest("Invalid status");
  db.prepare(`
    UPDATE tasks SET project_id=?, title=?, description=?, status=?, priority=?, position=?, assignee_id=?, due_date=?, updated_at=? WHERE id=?
  `).run(merged.project_id, merged.title, merged.description, merged.status, merged.priority,
    merged.position, merged.assignee_id, merged.due_date, now(), existing.id);
  indexEntity(a.workspaceId, "task", existing.id, merged.title, merged.description ?? "");
  audit(a.workspaceId, actor(c), "task.update", "task", existing.id, { status: merged.status });
  return c.json(taskRow(db.prepare("SELECT * FROM tasks WHERE id = ?").get(existing.id)));
});

projectRoutes.post("/tasks/:id/move", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const status = String(body.status ?? "") as TaskStatus;
  const position = Number(body.position ?? 0);
  if (!TASK_STATUSES.includes(status)) throw badRequest("Invalid status");
  const db = getDb();
  const existing = db.prepare("SELECT * FROM tasks WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Task not found");
  db.prepare("UPDATE tasks SET status = ?, position = ?, updated_at = ? WHERE id = ?").run(status, position, now(), existing.id);
  audit(a.workspaceId, actor(c), "task.move", "task", existing.id, { status, position });
  return c.json(taskRow(db.prepare("SELECT * FROM tasks WHERE id = ?").get(existing.id)));
});

projectRoutes.delete("/tasks/:id", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM tasks WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("task", param(c, "id"));
  audit(a.workspaceId, actor(c), "task.delete", "task", param(c, "id"));
  return c.json({ ok: true });
});

projectRoutes.get("/tasks/:id/comments", requireScope("projects:read", "viewer"), async (c) => {
  const rows = getDb().prepare("SELECT * FROM task_comments WHERE task_id = ? ORDER BY created_at").all(param(c, "id")) as any[];
  return c.json({ items: rows.map((r) => ({ id: r.id, taskId: r.task_id, authorId: r.author_id, body: r.body, createdAt: r.created_at })) });
});

projectRoutes.post("/tasks/:id/comments", requireScope("projects:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const text = String(body.body ?? "").trim();
  if (!text) throw badRequest("body is required");
  const id = newId("cmt");
  getDb().prepare("INSERT INTO task_comments (id, task_id, author_id, body, created_at) VALUES (?, ?, ?, ?, ?)")
    .run(id, param(c, "id"), a.userId, text, now());
  return c.json({ id }, 201);
});
