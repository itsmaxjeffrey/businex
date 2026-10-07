import { param } from "../context";
import { Hono } from "hono";
import { getDb, now, bool } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { indexEntity, removeFromIndex } from "../lib/search";
import { companySchema, contactSchema, dealSchema, DEAL_STAGES } from "@businex/shared";

export const crmRoutes = new Hono<Env>();
crmRoutes.use("*", authenticate, requireWorkspace);

const paging = (c: any) => ({
  limit: Math.min(Number(c.req.query("limit") ?? 50), 200),
  offset: Math.max(Number(c.req.query("offset") ?? 0), 0),
});

const companyRow = (r: any) => ({
  id: r.id, name: r.name, domain: r.domain, industry: r.industry, size: r.size,
  website: r.website, notes: r.notes, createdAt: r.created_at, updatedAt: r.updated_at,
});
const contactRow = (r: any) => ({
  id: r.id, firstName: r.first_name, lastName: r.last_name, email: r.email, phone: r.phone,
  title: r.title, companyId: r.company_id, notes: r.notes, createdAt: r.created_at, updatedAt: r.updated_at,
});
const dealRow = (r: any) => ({
  id: r.id, name: r.name, companyId: r.company_id, contactId: r.contact_id, stage: r.stage,
  value: r.value, currency: r.currency, closeDate: r.close_date, ownerId: r.owner_id,
  notes: r.notes, createdAt: r.created_at, updatedAt: r.updated_at,
});

// Companies -------------------------------------------------------------------

crmRoutes.get("/companies", requireScope("crm:read", "viewer"), async (c) => {
  const a = auth(c);
  const { limit, offset } = paging(c);
  const q = c.req.query("q");
  const db = getDb();
  const where = q ? "AND (name LIKE ? OR domain LIKE ?)" : "";
  const args: any[] = [a.workspaceId];
  if (q) args.push("%" + q + "%", "%" + q + "%");
  const total = (db.prepare("SELECT COUNT(*) AS n FROM companies WHERE workspace_id = ? " + where).get(...args) as any).n;
  const rows = db.prepare("SELECT * FROM companies WHERE workspace_id = ? " + where + " ORDER BY name LIMIT ? OFFSET ?")
    .all(...args, limit, offset) as any[];
  return c.json({ items: rows.map(companyRow), total, limit, offset });
});

crmRoutes.post("/companies", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = companySchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid company", parsed.error.flatten());
  const d = parsed.data;
  const id = newId("cmp");
  const t = now();
  getDb().prepare(`
    INSERT INTO companies (id, workspace_id, name, domain, industry, size, website, notes, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.name, d.domain ?? null, d.industry ?? null, d.size ?? null, d.website ?? null, d.notes ?? null, t, t);
  indexEntity(a.workspaceId, "company", id, d.name, [d.domain, d.industry, d.notes].filter(Boolean).join(" "));
  audit(a.workspaceId, actor(c), "company.create", "company", id, { name: d.name });
  return c.json(companyRow(getDb().prepare("SELECT * FROM companies WHERE id = ?").get(id)), 201);
});

crmRoutes.get("/companies/:id", requireScope("crm:read", "viewer"), async (c) => {
  const row = getDb().prepare("SELECT * FROM companies WHERE id = ? AND workspace_id = ?").get(param(c, "id"), auth(c).workspaceId) as any;
  if (!row) throw notFound("Company not found");
  return c.json(companyRow(row));
});

crmRoutes.patch("/companies/:id", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM companies WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Company not found");
  const parsed = companySchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid company", parsed.error.flatten());
  const d = { ...existing, ...Object.fromEntries(Object.entries(parsed.data).map(([k, v]) => [k, v ?? null])) };
  db.prepare(`
    UPDATE companies SET name=?, domain=?, industry=?, size=?, website=?, notes=?, updated_at=? WHERE id=?
  `).run(d.name, d.domain, d.industry, d.size, d.website, d.notes, now(), existing.id);
  indexEntity(a.workspaceId, "company", existing.id, d.name, [d.domain, d.industry, d.notes].filter(Boolean).join(" "));
  audit(a.workspaceId, actor(c), "company.update", "company", existing.id);
  return c.json(companyRow(db.prepare("SELECT * FROM companies WHERE id = ?").get(existing.id)));
});

crmRoutes.delete("/companies/:id", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM companies WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("company", param(c, "id"));
  audit(a.workspaceId, actor(c), "company.delete", "company", param(c, "id"));
  return c.json({ ok: true });
});

// Contacts --------------------------------------------------------------------

crmRoutes.get("/contacts", requireScope("crm:read", "viewer"), async (c) => {
  const a = auth(c);
  const { limit, offset } = paging(c);
  const q = c.req.query("q");
  const db = getDb();
  const where = q ? "AND (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?)" : "";
  const args: any[] = [a.workspaceId];
  if (q) args.push("%" + q + "%", "%" + q + "%", "%" + q + "%");
  const total = (db.prepare("SELECT COUNT(*) AS n FROM contacts WHERE workspace_id = ? " + where).get(...args) as any).n;
  const rows = db.prepare("SELECT * FROM contacts WHERE workspace_id = ? " + where + " ORDER BY last_name, first_name LIMIT ? OFFSET ?")
    .all(...args, limit, offset) as any[];
  return c.json({ items: rows.map(contactRow), total, limit, offset });
});

crmRoutes.post("/contacts", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = contactSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid contact", parsed.error.flatten());
  const d = parsed.data;
  const id = newId("cnt");
  const t = now();
  getDb().prepare(`
    INSERT INTO contacts (id, workspace_id, first_name, last_name, email, phone, title, company_id, notes, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.firstName, d.lastName, d.email ?? null, d.phone ?? null, d.title ?? null, d.companyId ?? null, d.notes ?? null, t, t);
  indexEntity(a.workspaceId, "contact", id, d.firstName + " " + d.lastName, [d.email, d.title, d.notes].filter(Boolean).join(" "));
  audit(a.workspaceId, actor(c), "contact.create", "contact", id, { name: d.firstName + " " + d.lastName });
  return c.json(contactRow(getDb().prepare("SELECT * FROM contacts WHERE id = ?").get(id)), 201);
});

crmRoutes.get("/contacts/:id", requireScope("crm:read", "viewer"), async (c) => {
  const row = getDb().prepare("SELECT * FROM contacts WHERE id = ? AND workspace_id = ?").get(param(c, "id"), auth(c).workspaceId) as any;
  if (!row) throw notFound("Contact not found");
  return c.json(contactRow(row));
});

crmRoutes.patch("/contacts/:id", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM contacts WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Contact not found");
  const parsed = contactSchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid contact", parsed.error.flatten());
  const map: Record<string, string> = {
    firstName: "first_name", lastName: "last_name", email: "email", phone: "phone",
    title: "title", companyId: "company_id", notes: "notes",
  };
  const merged = { ...existing };
  for (const [k, col] of Object.entries(map)) {
    if (k in parsed.data) merged[col] = (parsed.data as any)[k] ?? null;
  }
  db.prepare(`
    UPDATE contacts SET first_name=?, last_name=?, email=?, phone=?, title=?, company_id=?, notes=?, updated_at=? WHERE id=?
  `).run(merged.first_name, merged.last_name, merged.email, merged.phone, merged.title, merged.company_id, merged.notes, now(), existing.id);
  indexEntity(a.workspaceId, "contact", existing.id, merged.first_name + " " + merged.last_name, [merged.email, merged.title, merged.notes].filter(Boolean).join(" "));
  audit(a.workspaceId, actor(c), "contact.update", "contact", existing.id);
  return c.json(contactRow(db.prepare("SELECT * FROM contacts WHERE id = ?").get(existing.id)));
});

crmRoutes.delete("/contacts/:id", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM contacts WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("contact", param(c, "id"));
  audit(a.workspaceId, actor(c), "contact.delete", "contact", param(c, "id"));
  return c.json({ ok: true });
});

// Deals -----------------------------------------------------------------------

crmRoutes.get("/deals", requireScope("crm:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const rows = db.prepare("SELECT * FROM deals WHERE workspace_id = ? ORDER BY updated_at DESC").all(a.workspaceId) as any[];
  return c.json({ items: rows.map(dealRow) });
});

crmRoutes.post("/deals", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = dealSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid deal", parsed.error.flatten());
  const d = parsed.data;
  const id = newId("del");
  const t = now();
  getDb().prepare(`
    INSERT INTO deals (id, workspace_id, name, company_id, contact_id, stage, value, currency, close_date, owner_id, notes, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, d.name, d.companyId ?? null, d.contactId ?? null, d.stage, d.value, d.currency, d.closeDate ?? null, d.ownerId ?? null, d.notes ?? null, t, t);
  indexEntity(a.workspaceId, "deal", id, d.name, d.notes ?? "");
  audit(a.workspaceId, actor(c), "deal.create", "deal", id, { name: d.name, stage: d.stage });
  return c.json(dealRow(getDb().prepare("SELECT * FROM deals WHERE id = ?").get(id)), 201);
});

crmRoutes.patch("/deals/:id", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM deals WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Deal not found");
  const parsed = dealSchema.partial().safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid deal", parsed.error.flatten());
  const map: Record<string, string> = {
    name: "name", companyId: "company_id", contactId: "contact_id", stage: "stage",
    value: "value", currency: "currency", closeDate: "close_date", ownerId: "owner_id", notes: "notes",
  };
  const merged = { ...existing };
  for (const [k, col] of Object.entries(map)) if (k in parsed.data) merged[col] = (parsed.data as any)[k] ?? null;
  if (merged.stage && !DEAL_STAGES.includes(merged.stage)) throw badRequest("Invalid stage");
  db.prepare(`
    UPDATE deals SET name=?, company_id=?, contact_id=?, stage=?, value=?, currency=?, close_date=?, owner_id=?, notes=?, updated_at=? WHERE id=?
  `).run(merged.name, merged.company_id, merged.contact_id, merged.stage, merged.value, merged.currency, merged.close_date, merged.owner_id, merged.notes, now(), existing.id);
  indexEntity(a.workspaceId, "deal", existing.id, merged.name, merged.notes ?? "");
  audit(a.workspaceId, actor(c), "deal.update", "deal", existing.id, { stage: merged.stage });
  return c.json(dealRow(db.prepare("SELECT * FROM deals WHERE id = ?").get(existing.id)));
});

crmRoutes.delete("/deals/:id", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM deals WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("deal", param(c, "id"));
  audit(a.workspaceId, actor(c), "deal.delete", "deal", param(c, "id"));
  return c.json({ ok: true });
});

// Activities ------------------------------------------------------------------

crmRoutes.get("/activities", requireScope("crm:read", "viewer"), async (c) => {
  const a = auth(c);
  const entityType = c.req.query("entityType");
  const entityId = c.req.query("entityId");
  const db = getDb();
  const rows = entityType && entityId
    ? db.prepare("SELECT * FROM activities WHERE workspace_id = ? AND entity_type = ? AND entity_id = ? ORDER BY created_at DESC").all(a.workspaceId, entityType, entityId)
    : db.prepare("SELECT * FROM activities WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 100").all(a.workspaceId);
  return c.json({ items: (rows as any[]).map((r) => ({
    id: r.id, type: r.type, subject: r.subject, body: r.body, entityType: r.entity_type,
    entityId: r.entity_id, dueAt: r.due_at, doneAt: r.done_at, createdBy: r.created_by, createdAt: r.created_at,
  })) });
});

crmRoutes.post("/activities", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const subject = String(body.subject ?? "").trim();
  if (!subject) throw badRequest("subject is required");
  const id = newId("act");
  getDb().prepare(`
    INSERT INTO activities (id, workspace_id, type, subject, body, entity_type, entity_id, due_at, done_at, created_by, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(id, a.workspaceId, String(body.type ?? "note"), subject, body.body ?? null,
    String(body.entityType ?? "contact"), String(body.entityId ?? ""), body.dueAt ?? null, null, a.userId, now());
  audit(a.workspaceId, actor(c), "activity.create", "activity", id);
  return c.json({ id }, 201);
});

crmRoutes.post("/activities/:id/done", requireScope("crm:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("UPDATE activities SET done_at = ? WHERE id = ? AND workspace_id = ?").run(now(), param(c, "id"), a.workspaceId);
  return c.json({ ok: true });
});
