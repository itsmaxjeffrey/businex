import { param } from "../context";
import { Hono } from "hono";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound, conflict } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { indexEntity, removeFromIndex } from "../lib/search";
import { documentSchema } from "@businex/shared";

export const documentRoutes = new Hono<Env>();
documentRoutes.use("*", authenticate, requireWorkspace);

const docRow = (r: any) => ({
  id: r.id, title: r.title, slug: r.slug, body: r.body, parentId: r.parent_id,
  createdBy: r.created_by, createdAt: r.created_at, updatedAt: r.updated_at,
});

function slugify(title: string): string {
  return title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 64) || "doc";
}

function uniqueSlug(db: any, workspaceId: string, base: string): string {
  let slug = base, i = 2;
  while (db.prepare("SELECT 1 FROM documents WHERE workspace_id = ? AND slug = ?").get(workspaceId, slug)) slug = base + "-" + i++;
  return slug;
}

documentRoutes.get("/", requireScope("documents:read", "viewer"), async (c) => {
  const a = auth(c);
  const q = c.req.query("q");
  const db = getDb();
  if (q) {
    const rows = db.prepare(`
      SELECT d.* FROM documents d
      JOIN search_index s ON s.entity_type = 'document' AND s.entity_id = d.id
      WHERE s.workspace_id = ? AND search_index MATCH ?
      ORDER BY bm25(search_index) LIMIT 50
    `).all(a.workspaceId, '"' + q.replace(/"/g, '""') + '"*') as any[];
    return c.json({ items: rows.map(docRow) });
  }
  const rows = db.prepare("SELECT * FROM documents WHERE workspace_id = ? ORDER BY parent_id NULLS FIRST, title").all(a.workspaceId) as any[];
  return c.json({ items: rows.map(docRow) });
});

documentRoutes.post("/", requireScope("documents:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = documentSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid document", parsed.error.flatten());
  const d = parsed.data;
  const db = getDb();
  const id = newId("doc");
  const t = now();
  const slug = uniqueSlug(db, a.workspaceId, slugify(d.title));
  db.prepare(`
    INSERT INTO documents (id, workspace_id, title, slug, body, parent_id, created_by, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.title, slug, d.body, d.parentId ?? null, a.userId, t, t);
  indexEntity(a.workspaceId, "document", id, d.title, d.body);
  audit(a.workspaceId, actor(c), "document.create", "document", id, { title: d.title });
  return c.json(docRow(db.prepare("SELECT * FROM documents WHERE id = ?").get(id)), 201);
});

documentRoutes.get("/:id", requireScope("documents:read", "viewer"), async (c) => {
  const row = getDb().prepare("SELECT * FROM documents WHERE (id = ? OR slug = ?) AND workspace_id = ?")
    .get(param(c, "id"), param(c, "id"), auth(c).workspaceId) as any;
  if (!row) throw notFound("Document not found");
  return c.json(docRow(row));
});

documentRoutes.patch("/:id", requireScope("documents:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM documents WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Document not found");
  const parsed = documentSchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid document", parsed.error.flatten());
  const title = parsed.data.title ?? existing.title;
  const body = parsed.data.body ?? existing.body;
  const parentId = "parentId" in parsed.data ? parsed.data.parentId ?? null : existing.parent_id;
  db.prepare("UPDATE documents SET title=?, body=?, parent_id=?, updated_at=? WHERE id=?")
    .run(title, body, parentId, now(), existing.id);
  indexEntity(a.workspaceId, "document", existing.id, title, body);
  audit(a.workspaceId, actor(c), "document.update", "document", existing.id);
  return c.json(docRow(db.prepare("SELECT * FROM documents WHERE id = ?").get(existing.id)));
});

documentRoutes.delete("/:id", requireScope("documents:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM documents WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("document", param(c, "id"));
  audit(a.workspaceId, actor(c), "document.delete", "document", param(c, "id"));
  return c.json({ ok: true });
});
