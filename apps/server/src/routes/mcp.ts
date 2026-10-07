import { param } from "../context";
import { Hono } from "hono";
import { getDb, now } from "../db";
import { newId } from "../lib/id";
import { authenticate, auth, requireScope, type Env } from "../context";
import { search, indexEntity } from "../lib/search";
import { audit } from "../lib/audit";

/**
 * Minimal MCP (Model Context Protocol) server over HTTP JSON-RPC 2.0.
 * Authenticated with a scoped API key ("Authorization: Bearer bnx_...").
 *
 * Methods: initialize · tools/list · tools/call
 * Every tool is workspace-bound to the API key's workspace and audited.
 */
export const mcpRoutes = new Hono<Env>();
mcpRoutes.use("*", authenticate);

const tools = [
  {
    name: "businex.search",
    description: "Full-text search across Businex business records (contacts, companies, deals, tasks, documents, invoices, events).",
    inputSchema: { type: "object", properties: { query: { type: "string" }, limit: { type: "number" } }, required: ["query"] },
  },
  {
    name: "businex.contact.create",
    description: "Create a CRM contact.",
    inputSchema: { type: "object", properties: { firstName: { type: "string" }, lastName: { type: "string" }, email: { type: "string" }, title: { type: "string" }, notes: { type: "string" } }, required: ["firstName", "lastName"] },
  },
  {
    name: "businex.task.create",
    description: "Create a task, optionally in a project.",
    inputSchema: { type: "object", properties: { title: { type: "string" }, description: { type: "string" }, projectId: { type: "string" }, priority: { type: "string" }, dueDate: { type: "string" } }, required: ["title"] },
  },
  {
    name: "businex.document.write",
    description: "Create or update a knowledge-base document (markdown).",
    inputSchema: { type: "object", properties: { title: { type: "string" }, body: { type: "string" }, id: { type: "string" } }, required: ["title", "body"] },
  },
  {
    name: "businex.invoice.create",
    description: "Create an invoice with line items.",
    inputSchema: { type: "object", properties: { issueDate: { type: "string" }, dueDate: { type: "string" }, currency: { type: "string" }, items: { type: "array" }, notes: { type: "string" } }, required: ["issueDate", "dueDate", "items"] },
  },
  {
    name: "businex.channel.post",
    description: "Post a message to a Businex channel (open-tag style).",
    inputSchema: { type: "object", properties: { channelId: { type: "string" }, body: { type: "string" }, threadId: { type: "string" } }, required: ["channelId", "body"] },
  },
  {
    name: "businex.analytics.overview",
    description: "Get business overview metrics (pipeline, tasks, invoices, activity).",
    inputSchema: { type: "object", properties: {} },
  },
] as const;

mcpRoutes.post("/mcp", async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const { id, method, params } = body as { id?: unknown; method?: string; params?: any };
  const respond = (result: unknown) => c.json({ jsonrpc: "2.0", id: id ?? null, result });
  const fail = (code: number, message: string) => c.json({ jsonrpc: "2.0", id: id ?? null, error: { code, message } });

  switch (method) {
    case "initialize":
      return respond({
        protocolVersion: "2025-03-26",
        capabilities: { tools: {} },
        serverInfo: { name: "businex", version: "0.1.0" },
      });
    case "tools/list":
      return respond({ tools });
    case "tools/call": {
      const name = String(params?.name ?? "");
      const args = params?.arguments ?? {};
      try {
        const result = await callTool(a.workspaceId, a.userId ?? a.apiKeyId, name, args);
        audit(a.workspaceId, { type: a.actorType, id: a.userId ?? a.apiKeyId }, "mcp.call", "tool", name);
        return respond({ content: [{ type: "text", text: JSON.stringify(result) }] });
      } catch (e: any) {
        return fail(-32000, e?.message ?? "Tool execution failed");
      }
    }
    case "ping":
      return respond({});
    default:
      return fail(-32601, "Method not found: " + method);
  }
});

async function callTool(workspaceId: string, actorId: string | null, name: string, args: any): Promise<unknown> {
  const db = getDb();
  switch (name) {
    case "businex.search":
      return { hits: search(workspaceId, String(args.query ?? ""), Number(args.limit ?? 10)) };
    case "businex.contact.create": {
      const id = newId("cnt");
      const t = now();
      db.prepare(`
        INSERT INTO contacts (id, workspace_id, first_name, last_name, email, title, notes, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
      `).run(id, workspaceId, String(args.firstName), String(args.lastName), args.email ?? null, args.title ?? null, args.notes ?? null, t, t);
      indexEntity(workspaceId, "contact", id, args.firstName + " " + args.lastName, [args.email, args.notes].filter(Boolean).join(" "));
      return { id, ok: true };
    }
    case "businex.task.create": {
      const id = newId("tsk");
      const t = now();
      const maxPos = (db.prepare("SELECT COALESCE(MAX(position), 0) AS p FROM tasks WHERE workspace_id = ?").get(workspaceId) as any).p;
      db.prepare(`
        INSERT INTO tasks (id, workspace_id, project_id, title, description, status, priority, position, due_date, created_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, 'todo', ?, ?, ?, ?, ?, ?)
      `).run(id, workspaceId, args.projectId ?? null, String(args.title), args.description ?? null,
        args.priority ?? "medium", maxPos + 1, args.dueDate ?? null, actorId, t, t);
      indexEntity(workspaceId, "task", id, String(args.title), String(args.description ?? ""));
      return { id, ok: true };
    }
    case "businex.document.write": {
      const t = now();
      if (args.id) {
        const existing = db.prepare("SELECT 1 FROM documents WHERE id = ? AND workspace_id = ?").get(args.id, workspaceId);
        if (!existing) throw new Error("Document not found");
        db.prepare("UPDATE documents SET title = ?, body = ?, updated_at = ? WHERE id = ?")
          .run(String(args.title), String(args.body), t, args.id);
        indexEntity(workspaceId, "document", args.id, String(args.title), String(args.body));
        return { id: args.id, ok: true, updated: true };
      }
      const id = newId("doc");
      const slug = String(args.title).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 64) || "doc";
      db.prepare(`
        INSERT INTO documents (id, workspace_id, title, slug, body, created_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(id, workspaceId, String(args.title), slug, String(args.body), actorId, t, t);
      indexEntity(workspaceId, "document", id, String(args.title), String(args.body));
      return { id, ok: true };
    }
    case "businex.invoice.create": {
      const items: any[] = Array.isArray(args.items) ? args.items : [];
      if (!items.length) throw new Error("items must be a non-empty array");
      const id = newId("inv");
      const t = now();
      const year = new Date().getFullYear();
      const count = (db.prepare("SELECT COUNT(*) AS n FROM invoices WHERE workspace_id = ?").get(workspaceId) as any).n;
      const number = "INV-" + year + "-" + String(count + 1).padStart(4, "0");
      const subtotal = items.reduce((sum, i) => sum + Number(i.quantity ?? 1) * Number(i.unitPrice ?? 0), 0);
      const taxRate = Number(args.taxRate ?? 0);
      db.prepare(`
        INSERT INTO invoices (id, workspace_id, number, status, issue_date, due_date, currency, subtotal, tax_rate, total, notes, created_by, created_at, updated_at)
        VALUES (?, ?, ?, 'draft', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `).run(id, workspaceId, number, String(args.issueDate), String(args.dueDate), String(args.currency ?? "USD"),
        subtotal, taxRate, subtotal * (1 + taxRate), args.notes ?? null, actorId, t, t);
      items.forEach((item, idx) => {
        const amount = Number(item.quantity ?? 1) * Number(item.unitPrice ?? 0);
        db.prepare(`
          INSERT INTO invoice_items (id, invoice_id, description, quantity, unit_price, amount, position)
          VALUES (?, ?, ?, ?, ?, ?, ?)
        `).run(newId("iit"), id, String(item.description ?? "Item"), Number(item.quantity ?? 1), Number(item.unitPrice ?? 0), amount, idx);
      });
      indexEntity(workspaceId, "invoice", id, number, args.notes ?? "");
      return { id, number, total: subtotal * (1 + taxRate), ok: true };
    }
    case "businex.channel.post": {
      const channel = db.prepare("SELECT 1 FROM channels WHERE id = ? AND workspace_id = ?").get(args.channelId, workspaceId);
      if (!channel) throw new Error("Channel not found");
      const id = newId("msg");
      db.prepare(`
        INSERT INTO messages (id, workspace_id, channel_id, thread_id, author_type, author_id, body, created_at)
        VALUES (?, ?, ?, ?, 'agent', ?, ?, ?)
      `).run(id, workspaceId, args.channelId, args.threadId ?? null, actorId, String(args.body), now());
      return { id, ok: true };
    }
    case "businex.analytics.overview": {
      const one = (sql: string) => (db.prepare(sql).get(workspaceId) as any);
      return {
        contacts: one("SELECT COUNT(*) AS n FROM contacts WHERE workspace_id = ?").n,
        companies: one("SELECT COUNT(*) AS n FROM companies WHERE workspace_id = ?").n,
        openDeals: one("SELECT COUNT(*) AS n FROM deals WHERE workspace_id = ? AND stage NOT IN ('won','lost')").n,
        openTasks: one("SELECT COUNT(*) AS n FROM tasks WHERE workspace_id = ? AND status != 'done'").n,
        openInvoices: one("SELECT COALESCE(SUM(total), 0) AS total FROM invoices WHERE workspace_id = ? AND status IN ('sent','overdue')").total,
      };
    }
    default:
      throw new Error("Unknown tool: " + name);
  }
}
