import { param } from "../context";
import { Hono } from "hono";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { badRequest, notFound } from "../lib/errors";
import { authenticate, auth, requireWorkspace, requireScope, actor, type Env } from "../context";
import { audit } from "../lib/audit";
import { indexEntity, removeFromIndex } from "../lib/search";
import { invoiceSchema, INVOICE_STATUSES } from "@businex/shared";

export const invoiceRoutes = new Hono<Env>();
invoiceRoutes.use("*", authenticate, requireWorkspace);

const invoiceRow = (r: any, items?: any[]) => ({
  id: r.id, number: r.number, companyId: r.company_id, contactId: r.contact_id, status: r.status,
  issueDate: r.issue_date, dueDate: r.due_date, currency: r.currency, subtotal: r.subtotal,
  taxRate: r.tax_rate, total: r.total, notes: r.notes, createdBy: r.created_by,
  createdAt: r.created_at, updatedAt: r.updated_at,
  items: items?.map((i) => ({
    id: i.id, invoiceId: i.invoice_id, description: i.description,
    quantity: i.quantity, unitPrice: i.unit_price, amount: i.amount, position: i.position,
  })),
});

function nextInvoiceNumber(db: any, workspaceId: string): string {
  const year = new Date().getFullYear();
  const row = db.prepare("SELECT COUNT(*) AS n FROM invoices WHERE workspace_id = ?").get(workspaceId) as any;
  return "INV-" + year + "-" + String(row.n + 1).padStart(4, "0");
}

function invoiceItems(db: any, invoiceId: string): any[] {
  return db.prepare("SELECT * FROM invoice_items WHERE invoice_id = ? ORDER BY position").all(invoiceId) as any[];
}

invoiceRoutes.get("/", requireScope("invoices:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const status = c.req.query("status");
  const rows = (status
    ? db.prepare("SELECT * FROM invoices WHERE workspace_id = ? AND status = ? ORDER BY issue_date DESC").all(a.workspaceId, status)
    : db.prepare("SELECT * FROM invoices WHERE workspace_id = ? ORDER BY issue_date DESC").all(a.workspaceId)) as any[];
  return c.json({ items: rows.map((r) => invoiceRow(r)) });
});

invoiceRoutes.post("/", requireScope("invoices:write", "member"), async (c) => {
  const a = auth(c);
  const parsed = invoiceSchema.safeParse(await c.req.json().catch(() => ({})));
  if (!parsed.success) throw badRequest("Invalid invoice", parsed.error.flatten());
  const d = parsed.data;
  const db = getDb();
  const id = newId("inv");
  const t = now();
  const number = nextInvoiceNumber(db, a.workspaceId);
  const subtotal = d.items.reduce((sum, i) => sum + i.quantity * i.unitPrice, 0);
  const total = subtotal * (1 + d.taxRate);

  const run = db.transaction(() => {
    db.prepare(`
      INSERT INTO invoices (id, workspace_id, number, company_id, contact_id, status, issue_date, due_date, currency, subtotal, tax_rate, total, notes, created_by, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(id, a.workspaceId, number, d.companyId ?? null, d.contactId ?? null, d.status,
      d.issueDate, d.dueDate, d.currency, subtotal, d.taxRate, total, d.notes ?? null, a.userId, t, t);
    d.items.forEach((item, idx) => {
      db.prepare(`
        INSERT INTO invoice_items (id, invoice_id, description, quantity, unit_price, amount, position)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `).run(newId("iit"), id, item.description, item.quantity, item.unitPrice, item.quantity * item.unitPrice, idx);
    });
  });
  run();
  indexEntity(a.workspaceId, "invoice", id, number, d.notes ?? "");
  audit(a.workspaceId, actor(c), "invoice.create", "invoice", id, { number, total });
  return c.json(invoiceRow(db.prepare("SELECT * FROM invoices WHERE id = ?").get(id), invoiceItems(db, id)), 201);
});

invoiceRoutes.get("/:id", requireScope("invoices:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const row = db.prepare("SELECT * FROM invoices WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!row) throw notFound("Invoice not found");
  return c.json(invoiceRow(row, invoiceItems(db, row.id)));
});

invoiceRoutes.patch("/:id", requireScope("invoices:write", "member"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const existing = db.prepare("SELECT * FROM invoices WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!existing) throw notFound("Invoice not found");
  const body = await c.req.json().catch(() => ({}));
  const status = body.status ? String(body.status) : existing.status;
  if (!INVOICE_STATUSES.includes(status)) throw badRequest("Invalid status");

  const run = db.transaction(() => {
    db.prepare(`
      UPDATE invoices SET status=?, company_id=?, contact_id=?, issue_date=?, due_date=?, currency=?, tax_rate=?, notes=?, updated_at=? WHERE id=?
    `).run(status,
      body.companyId !== undefined ? body.companyId ?? null : existing.company_id,
      body.contactId !== undefined ? body.contactId ?? null : existing.contact_id,
      body.issueDate ?? existing.issue_date, body.dueDate ?? existing.due_date,
      body.currency ?? existing.currency,
      body.taxRate !== undefined ? Number(body.taxRate) : existing.tax_rate,
      body.notes !== undefined ? body.notes ?? null : existing.notes,
      now(), existing.id);
    if (Array.isArray(body.items)) {
      db.prepare("DELETE FROM invoice_items WHERE invoice_id = ?").run(existing.id);
      let subtotal = 0;
      body.items.forEach((item: any, idx: number) => {
        const amount = Number(item.quantity) * Number(item.unitPrice);
        subtotal += amount;
        db.prepare(`
          INSERT INTO invoice_items (id, invoice_id, description, quantity, unit_price, amount, position)
          VALUES (?, ?, ?, ?, ?, ?, ?)
        `).run(newId("iit"), existing.id, String(item.description), Number(item.quantity), Number(item.unitPrice), amount, idx);
      });
      const taxRate = body.taxRate !== undefined ? Number(body.taxRate) : existing.tax_rate;
      db.prepare("UPDATE invoices SET subtotal = ?, total = ? WHERE id = ?").run(subtotal, subtotal * (1 + taxRate), existing.id);
    }
  });
  run();
  audit(a.workspaceId, actor(c), "invoice.update", "invoice", existing.id, { status });
  return c.json(invoiceRow(db.prepare("SELECT * FROM invoices WHERE id = ?").get(existing.id), invoiceItems(db, existing.id)));
});

invoiceRoutes.delete("/:id", requireScope("invoices:write", "member"), async (c) => {
  const a = auth(c);
  getDb().prepare("DELETE FROM invoices WHERE id = ? AND workspace_id = ?").run(param(c, "id"), a.workspaceId);
  removeFromIndex("invoice", param(c, "id"));
  audit(a.workspaceId, actor(c), "invoice.delete", "invoice", param(c, "id"));
  return c.json({ ok: true });
});

/** Printable invoice rendering (HTML; the client can print or save as PDF). */
invoiceRoutes.get("/:id/print", requireScope("invoices:read", "viewer"), async (c) => {
  const a = auth(c);
  const db = getDb();
  const row = db.prepare("SELECT * FROM invoices WHERE id = ? AND workspace_id = ?").get(param(c, "id"), a.workspaceId) as any;
  if (!row) throw notFound("Invoice not found");
  const ws = db.prepare("SELECT name FROM workspaces WHERE id = ?").get(a.workspaceId) as any;
  const items = invoiceItems(db, row.id);
  const esc = (s: unknown) => String(s ?? "").replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[ch]!));
  const html = [
    "<!doctype html><html><head><meta charset='utf-8'><title>" + esc(row.number) + "</title>",
    "<style>body{font-family:system-ui,sans-serif;max-width:720px;margin:40px auto;color:#0f172a}",
    "table{width:100%;border-collapse:collapse;margin:24px 0}td,th{padding:8px 12px;border-bottom:1px solid #e2e8f0;text-align:left}",
    ".total{font-weight:700;font-size:1.1em}.muted{color:#64748b}</style></head><body>",
    "<h1>" + esc(row.number) + "</h1><p class='muted'>" + esc(ws?.name ?? "") + "</p>",
    "<p>Issue date: " + esc(row.issue_date) + " · Due: " + esc(row.due_date) + " · Status: " + esc(row.status) + "</p>",
    "<table><tr><th>Item</th><th>Qty</th><th>Unit price</th><th>Amount</th></tr>",
    ...items.map((i) => "<tr><td>" + esc(i.description) + "</td><td>" + i.quantity + "</td><td>" + i.unit_price + "</td><td>" + i.amount + "</td></tr>"),
    "</table><p>Subtotal: " + row.subtotal + " " + esc(row.currency) + "</p>",
    "<p>Tax: " + (row.tax_rate * 100).toFixed(1) + "%</p>",
    "<p class='total'>Total: " + row.total + " " + esc(row.currency) + "</p>",
    row.notes ? "<p class='muted'>" + esc(row.notes) + "</p>" : "",
    "</body></html>",
  ].join("");
  return c.html(html);
});
