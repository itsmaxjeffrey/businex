import { param } from "../context";
import { Hono } from "hono";
import { getDb } from "../db";
import { authenticate, auth, requireWorkspace, requireScope, type Env } from "../context";

export const analyticsRoutes = new Hono<Env>();
analyticsRoutes.use("*", authenticate, requireWorkspace);

/**
 * Dashboard metrics in a single round trip: revenue pipeline, work in flight,
 * upcoming commitments, and agent activity.
 */
analyticsRoutes.get("/analytics/overview", requireScope("analytics:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const one = (sql: string, ...args: any[]) => (db.prepare(sql).get(...args) as any);

  const pipeline = db.prepare(`
    SELECT stage, COUNT(*) AS count, COALESCE(SUM(value), 0) AS value
    FROM deals WHERE workspace_id = ? GROUP BY stage
  `).all(a.workspaceId) as any[];

  const tasksByStatus = db.prepare(`
    SELECT status, COUNT(*) AS count FROM tasks WHERE workspace_id = ? GROUP BY status
  `).all(a.workspaceId) as any[];

  const overdueTasks = one(`
    SELECT COUNT(*) AS n FROM tasks
    WHERE workspace_id = ? AND due_date IS NOT NULL AND due_date < ? AND status != 'done'
  `, a.workspaceId, new Date().toISOString().slice(0, 10)).n;

  const openInvoices = one(`
    SELECT COUNT(*) AS n, COALESCE(SUM(total), 0) AS total
    FROM invoices WHERE workspace_id = ? AND status IN ('sent', 'overdue')
  `, a.workspaceId);

  const upcomingEvents = db.prepare(`
    SELECT id, title, starts_at, ends_at, color FROM events
    WHERE workspace_id = ? AND starts_at >= ? ORDER BY starts_at LIMIT 8
  `).all(a.workspaceId, new Date().toISOString()) as any[];

  const recentActivity = db.prepare(`
    SELECT action, entity_type, entity_id, actor_type, created_at FROM audit_log
    WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 12
  `).all(a.workspaceId) as any[];

  const counts = {
    contacts: one("SELECT COUNT(*) AS n FROM contacts WHERE workspace_id = ?", a.workspaceId).n,
    companies: one("SELECT COUNT(*) AS n FROM companies WHERE workspace_id = ?", a.workspaceId).n,
    projects: one("SELECT COUNT(*) AS n FROM projects WHERE workspace_id = ?", a.workspaceId).n,
    documents: one("SELECT COUNT(*) AS n FROM documents WHERE workspace_id = ?", a.workspaceId).n,
    messages: one("SELECT COUNT(*) AS n FROM messages WHERE workspace_id = ?", a.workspaceId).n,
    agents: one("SELECT COUNT(*) AS n FROM agents WHERE workspace_id = ?", a.workspaceId).n,
  };

  return c.json({
    counts,
    pipeline: pipeline.map((r) => ({ stage: r.stage, count: r.count, value: r.value })),
    tasksByStatus: tasksByStatus.map((r) => ({ status: r.status, count: r.count })),
    overdueTasks,
    openInvoices: { count: openInvoices.n, total: openInvoices.total },
    upcomingEvents: upcomingEvents.map((r) => ({ id: r.id, title: r.title, startsAt: r.starts_at, endsAt: r.ends_at, color: r.color })),
    recentActivity: recentActivity.map((r) => ({ action: r.action, entityType: r.entity_type, entityId: r.entity_id, actorType: r.actor_type, createdAt: r.created_at })),
  });
});

analyticsRoutes.get("/analytics/pipeline-history", requireScope("analytics:read", "viewer"), async (c) => {
  const a = auth(c);
  const rows = getDb().prepare(`
    SELECT substr(created_at, 1, 7) AS month, COUNT(*) AS deals, COALESCE(SUM(value), 0) AS value
    FROM deals WHERE workspace_id = ? GROUP BY month ORDER BY month LIMIT 24
  `).all(a.workspaceId) as any[];
  return c.json({ items: rows.map((r) => ({ month: r.month, deals: r.deals, value: r.value })) });
});
