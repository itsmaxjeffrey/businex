// Demo mode: runs the real Businex UI against an in-memory workspace so the
// product can be explored in a browser with no backend (e.g. on businex.app).
// Enabled with ?demo=1 or when the app is served under /app (site preview).

export function isDemoMode(): boolean {
  if (typeof location === "undefined") return false;
  return new URLSearchParams(location.search).has("demo") || location.pathname.startsWith("/app");
}

const now = Date.now();
const iso = (offsetMs: number) => new Date(now + offsetMs).toISOString();
const day = 86400000;

function id(prefix: string, n: number): string {
  return prefix + "_demo" + String(n).padStart(4, "0");
}

const store = {
  user: {
    id: id("usr", 1),
    email: "demo@businex.app",
    name: "Alex Rivera",
    avatarColor: "#ff6b35",
    createdAt: iso(-90 * day),
    lastSeenAt: iso(-5 * 60000),
  },
  workspace: {
    id: id("ws", 1),
    orgId: id("org", 1),
    name: "Northwind Labs",
    slug: "northwind-labs",
    createdAt: iso(-90 * day),
    role: "owner",
  },
  contacts: [
    { id: id("cnt", 1), firstName: "Anna", lastName: "Keller", email: "anna@northwind.example", phone: "+41 44 000 11 22", title: "CTO", companyId: id("cmp", 1), notes: "Met at Demo Day.", createdAt: iso(-40 * day), updatedAt: iso(-3 * day) },
    { id: id("cnt", 2), firstName: "Marco", lastName: "Bianchi", email: "marco@acme.example", phone: "+39 02 000 33 44", title: "Head of Ops", companyId: id("cmp", 2), notes: null, createdAt: iso(-30 * day), updatedAt: iso(-2 * day) },
    { id: id("cnt", 3), firstName: "Priya", lastName: "Sharma", email: "priya@lumen.example", title: "Founder", companyId: id("cmp", 3), notes: "Wants Q1 kickoff.", createdAt: iso(-12 * day), updatedAt: iso(-1 * day) },
    { id: id("cnt", 4), firstName: "Jonas", lastName: "Meier", email: "jonas@vertex.example", title: "CFO", companyId: id("cmp", 2), notes: null, createdAt: iso(-6 * day), updatedAt: iso(-6 * day) },
  ],
  companies: [
    { id: id("cmp", 1), name: "Northwind Labs", domain: "northwind.example", industry: "Software", size: "11-50", website: "https://northwind.example", notes: "Our own company.", createdAt: iso(-90 * day), updatedAt: iso(-2 * day) },
    { id: id("cmp", 2), name: "Acme Group", domain: "acme.example", industry: "Manufacturing", size: "200-500", website: "https://acme.example", notes: null, createdAt: iso(-45 * day), updatedAt: iso(-8 * day) },
    { id: id("cmp", 3), name: "Lumen Studio", domain: "lumen.example", industry: "Design", size: "2-10", website: "https://lumen.example", notes: null, createdAt: iso(-12 * day), updatedAt: iso(-12 * day) },
  ],
  deals: [
    { id: id("del", 1), name: "Acme platform rollout", companyId: id("cmp", 2), contactId: id("cnt", 2), stage: "proposal", value: 48000, currency: "USD", closeDate: iso(21 * day), ownerId: id("usr", 1), notes: null, createdAt: iso(-20 * day), updatedAt: iso(-2 * day) },
    { id: id("del", 2), name: "Lumen brand system", companyId: id("cmp", 3), contactId: id("cnt", 3), stage: "qualified", value: 18500, currency: "USD", closeDate: iso(35 * day), ownerId: id("usr", 1), notes: null, createdAt: iso(-10 * day), updatedAt: iso(-1 * day) },
    { id: id("del", 3), name: "Vertex audit automation", companyId: id("cmp", 2), contactId: id("cnt", 4), stage: "negotiation", value: 72000, currency: "USD", closeDate: iso(12 * day), ownerId: id("usr", 1), notes: "Legal reviewing terms.", createdAt: iso(-25 * day), updatedAt: iso(-4 * day) },
    { id: id("del", 4), name: "Northwind internal tooling", companyId: id("cmp", 1), contactId: id("cnt", 1), stage: "won", value: 24000, currency: "USD", closeDate: iso(-8 * day), ownerId: id("usr", 1), notes: null, createdAt: iso(-60 * day), updatedAt: iso(-8 * day) },
    { id: id("del", 5), name: "Retail pilot", companyId: id("cmp", 2), contactId: id("cnt", 2), stage: "lead", value: 9000, currency: "USD", closeDate: iso(50 * day), ownerId: id("usr", 1), notes: null, createdAt: iso(-3 * day), updatedAt: iso(-3 * day) },
  ],
  projects: [
    { id: id("prj", 1), name: "Website Redesign", key: "WEB", description: "New marketing site + docs.", status: "active", color: "#ff6b35", dueDate: iso(30 * day), createdBy: id("usr", 1), createdAt: iso(-40 * day), updatedAt: iso(-2 * day) },
    { id: id("prj", 2), name: "Agent Playbook", key: "AGENT", description: "OpenClaw + open-tag rollout.", status: "active", color: "#7fb77e", dueDate: iso(60 * day), createdBy: id("usr", 1), createdAt: iso(-20 * day), updatedAt: iso(-1 * day) },
  ],
  tasks: [
    { id: id("tsk", 1), projectId: id("prj", 1), title: "Draft hero copy", description: null, status: "done", priority: "high", position: 1, assigneeId: id("usr", 1), dueDate: iso(-2 * day), messageId: null, createdBy: id("usr", 1), createdAt: iso(-9 * day), updatedAt: iso(-2 * day) },
    { id: id("tsk", 2), projectId: id("prj", 1), title: "Design module grid", description: null, status: "in_progress", priority: "medium", position: 2, assigneeId: id("usr", 1), dueDate: iso(2 * day), messageId: null, createdBy: id("usr", 1), createdAt: iso(-7 * day), updatedAt: iso(-1 * day) },
    { id: id("tsk", 3), projectId: id("prj", 1), title: "Ship pricing page", description: null, status: "review", priority: "high", position: 3, assigneeId: id("usr", 1), dueDate: iso(4 * day), messageId: null, createdBy: id("usr", 1), createdAt: iso(-5 * day), updatedAt: iso(-1 * day) },
    { id: id("tsk", 4), projectId: id("prj", 2), title: "Wire OpenClaw bridge", description: null, status: "todo", priority: "urgent", position: 4, assigneeId: id("usr", 1), dueDate: iso(6 * day), messageId: null, createdBy: id("usr", 1), createdAt: iso(-3 * day), updatedAt: iso(-3 * day) },
    { id: id("tsk", 5), projectId: id("prj", 2), title: "Tag channels with open-tag schema", description: null, status: "backlog", priority: "low", position: 5, assigneeId: null, dueDate: null, messageId: null, createdBy: id("usr", 1), createdAt: iso(-2 * day), updatedAt: iso(-2 * day) },
    { id: id("tsk", 6), projectId: null, title: "Prepare Q4 board deck", description: null, status: "todo", priority: "medium", position: 6, assigneeId: id("usr", 1), dueDate: iso(8 * day), messageId: null, createdBy: id("usr", 1), createdAt: iso(-1 * day), updatedAt: iso(-1 * day) },
  ],
  documents: [
    { id: id("doc", 1), title: "Onboarding playbook", slug: "onboarding-playbook", body: "# Onboarding playbook\n\nHow we bring a new customer live in under two weeks.\n\n## Steps\n\n- Kickoff call and workspace setup\n- Import contacts and open deals\n- Connect **OpenClaw** agents to the channels\n- First weekly review", parentId: null, createdBy: id("usr", 1), createdAt: iso(-30 * day), updatedAt: iso(-2 * day) },
    { id: id("doc", 2), title: "Pricing v3", slug: "pricing-v3", body: "# Pricing v3\n\nThree tiers: **Starter**, **Team**, **Scale**.\n\nTeam includes the agent runtime and channel integrations.", parentId: null, createdBy: id("usr", 1), createdAt: iso(-15 * day), updatedAt: iso(-4 * day) },
    { id: id("doc", 3), title: "Agent guidelines", slug: "agent-guidelines", body: "# Agent guidelines\n\nAgents are teammates. Every dispatch is audited, every reply lands in the thread it came from.", parentId: null, createdBy: id("usr", 1), createdAt: iso(-8 * day), updatedAt: iso(-1 * day) },
  ],
  events: [
    { id: id("evt", 1), title: "Acme rollout review", description: null, startsAt: iso(1 * day), endsAt: iso(1 * day + 3600000), allDay: false, location: "Zoom", color: "#ff6b35", createdBy: id("usr", 1), createdAt: iso(-5 * day), updatedAt: iso(-5 * day) },
    { id: id("evt", 2), title: "Board meeting", description: null, startsAt: iso(3 * day), endsAt: iso(3 * day + 7200000), allDay: false, location: "HQ", color: "#6fa8dc", createdBy: id("usr", 1), createdAt: iso(-5 * day), updatedAt: iso(-5 * day) },
    { id: id("evt", 3), title: "Agent ops sprint", description: null, startsAt: iso(6 * day), endsAt: iso(8 * day), allDay: true, location: null, color: "#7fb77e", createdBy: id("usr", 1), createdAt: iso(-2 * day), updatedAt: iso(-2 * day) },
    { id: id("evt", 4), title: "Lumen kickoff", description: null, startsAt: iso(10 * day), endsAt: iso(10 * day + 3600000), allDay: false, location: "Milan", color: "#e3b23c", createdBy: id("usr", 1), createdAt: iso(-1 * day), updatedAt: iso(-1 * day) },
  ],
  invoices: [
    { id: id("inv", 1), number: "INV-2026-0001", companyId: id("cmp", 2), contactId: id("cnt", 2), status: "paid", issueDate: iso(-30 * day).slice(0, 10), dueDate: iso(-2 * day).slice(0, 10), currency: "USD", subtotal: 4800, taxRate: 0.1, total: 5280, notes: null, createdBy: id("usr", 1), createdAt: iso(-30 * day), updatedAt: iso(-3 * day), items: [{ id: id("iit", 1), invoiceId: id("inv", 1), description: "Platform rollout — phase 1", quantity: 1, unitPrice: 4800, amount: 4800, position: 0 }] },
    { id: id("inv", 2), number: "INV-2026-0002", companyId: id("cmp", 3), contactId: id("cnt", 3), status: "sent", issueDate: iso(-6 * day).slice(0, 10), dueDate: iso(24 * day).slice(0, 10), currency: "USD", subtotal: 18500, taxRate: 0.1, total: 20350, notes: null, createdBy: id("usr", 1), createdAt: iso(-6 * day), updatedAt: iso(-6 * day), items: [{ id: id("iit", 2), invoiceId: id("inv", 2), description: "Brand system — deposit", quantity: 1, unitPrice: 18500, amount: 18500, position: 0 }] },
    { id: id("inv", 3), number: "INV-2026-0003", companyId: id("cmp", 2), contactId: id("cnt", 4), status: "draft", issueDate: iso(1 * day).slice(0, 10), dueDate: iso(31 * day).slice(0, 10), currency: "USD", subtotal: 2400, taxRate: 0.1, total: 2640, notes: null, createdBy: id("usr", 1), createdAt: iso(-1 * day), updatedAt: iso(-1 * day), items: [{ id: id("iit", 3), invoiceId: id("inv", 3), description: "Automation workshop", quantity: 2, unitPrice: 1200, amount: 2400, position: 0 }] },
  ],
  channels: [
    { id: id("chn", 1), kind: "channel", name: "general", topic: "Company-wide chatter", isPrivate: false, createdBy: id("usr", 1), createdAt: iso(-90 * day) },
    { id: id("chn", 2), kind: "channel", name: "acme-rollout", topic: "Acme platform rollout", isPrivate: false, createdBy: id("usr", 1), createdAt: iso(-25 * day) },
    { id: id("chn", 3), kind: "channel", name: "agents", topic: "Agent dispatches and reports", isPrivate: false, createdBy: id("usr", 1), createdAt: iso(-18 * day) },
  ],
  messages: [
    { id: id("msg", 1), channelId: id("chn", 1), threadId: null, authorType: "user", authorId: id("usr", 1), body: "Welcome to Businex — this is a live demo workspace with seeded data.", meta: null, createdAt: iso(-8 * day) },
    { id: id("msg", 2), channelId: id("chn", 2), threadId: null, authorType: "user", authorId: id("usr", 1), body: "@ops-bot summarize where the Acme rollout stands", meta: null, createdAt: iso(-2 * day) },
    { id: id("msg", 3), channelId: id("chn", 2), threadId: id("msg", 2), authorType: "agent", authorId: id("agt", 1), body: "Acme rollout is in the proposal stage (48k). Two tasks open: module grid in review, pricing page shipping this week. Next milestone: phase 1 sign-off on Friday.", meta: { via: "openclaw" }, createdAt: iso(-2 * day + 90000) },
    { id: id("msg", 4), channelId: id("chn", 3), threadId: null, authorType: "user", authorId: id("usr", 1), body: "New playbook live: agents get channel context on mention and report back in-thread.", meta: null, createdAt: iso(-1 * day) },
  ],
  agents: [
    { id: id("agt", 1), name: "ops-bot", kind: "openclaw", status: "idle", config: {}, createdAt: iso(-18 * day), updatedAt: iso(-2 * day) },
    { id: id("agt", 2), name: "sdr-bot", kind: "open-tag", status: "idle", config: {}, createdAt: iso(-12 * day), updatedAt: iso(-1 * day) },
    { id: id("agt", 3), name: "docs-bot", kind: "builtin", status: "working", config: {}, createdAt: iso(-9 * day), updatedAt: iso(-1 * day) },
  ],
  agentEvents: [
    { id: id("aev", 1), agentId: id("agt", 1), kind: "mention", payload: { body: "@ops-bot summarize where the Acme rollout stands" }, createdAt: iso(-2 * day) },
    { id: id("aev", 2), agentId: id("agt", 3), kind: "mention", payload: { body: "index the new docs" }, createdAt: iso(-1 * day) },
  ],
  members: [
    { id: id("mem", 1), role: "owner", createdAt: iso(-90 * day), user: { id: id("usr", 1), name: "Alex Rivera", email: "demo@businex.app", avatarColor: "#ff6b35" } },
    { id: id("mem", 2), role: "admin", createdAt: iso(-40 * day), user: { id: id("usr", 2), name: "Sam Okafor", email: "sam@northwind.example", avatarColor: "#6fa8dc" } },
    { id: id("mem", 3), role: "member", createdAt: iso(-20 * day), user: { id: id("usr", 3), name: "Lena Fischer", email: "lena@northwind.example", avatarColor: "#7fb77e" } },
  ],
  apiKeys: [
    { id: id("key", 1), name: "openclaw-agent", prefix: "bnx_a1b2c3", scopes: ["*"], createdBy: id("usr", 1), createdAt: iso(-15 * day), lastUsedAt: iso(-2 * 3600000), revokedAt: null },
    { id: id("key", 2), name: "reporting-readonly", prefix: "bnx_d4e5f6", scopes: ["analytics:read", "crm:read"], createdBy: id("usr", 1), createdAt: iso(-7 * day), lastUsedAt: null, revokedAt: null },
  ],
  audit: [
    { id: id("aud", 1), actorType: "agent", actorId: id("agt", 1), action: "task.update", entityType: "task", entityId: id("tsk", 2), meta: null, createdAt: iso(-2 * 3600000) },
    { id: id("aud", 2), actorType: "user", actorId: id("usr", 1), action: "invoice.create", entityType: "invoice", entityId: id("inv", 3), meta: null, createdAt: iso(-1 * day) },
    { id: id("aud", 3), actorType: "user", actorId: id("usr", 1), action: "document.update", entityType: "document", entityId: id("doc", 3), meta: null, createdAt: iso(-1 * day) },
  ],
};

let seq = 100;
const nextId = (prefix: string) => id(prefix, ++seq);

function overview() {
  const pipeline = ["lead", "qualified", "proposal", "negotiation", "won", "lost"].map((stage) => ({
    stage,
    count: store.deals.filter((d) => d.stage === stage).length,
    value: store.deals.filter((d) => d.stage === stage).reduce((s, d) => s + d.value, 0),
  }));
  const statuses = ["backlog", "todo", "in_progress", "review", "done"];
  return {
    counts: {
      contacts: store.contacts.length,
      companies: store.companies.length,
      projects: store.projects.length,
      documents: store.documents.length,
      messages: store.messages.length,
      agents: store.agents.length,
    },
    pipeline,
    tasksByStatus: statuses.map((status) => ({ status, count: store.tasks.filter((t) => t.status === status).length })),
    overdueTasks: store.tasks.filter((t) => t.dueDate && t.dueDate < new Date().toISOString().slice(0, 10) && t.status !== "done").length,
    openInvoices: {
      count: store.invoices.filter((i) => i.status === "sent" || i.status === "overdue").length,
      total: store.invoices.filter((i) => i.status === "sent" || i.status === "overdue").reduce((s, i) => s + i.total, 0),
    },
    upcomingEvents: store.events.slice(0, 5).map((e) => ({ id: e.id, title: e.title, startsAt: e.startsAt, endsAt: e.endsAt, color: e.color })),
    recentActivity: [...store.audit].reverse().map((a) => ({ action: a.action, entityType: a.entityType, entityId: a.entityId, actorType: a.actorType, createdAt: a.createdAt })),
  };
}

function search(query: string) {
  const q = query.toLowerCase();
  const hits: any[] = [];
  for (const d of store.documents) if ((d.title + d.body).toLowerCase().includes(q)) hits.push({ entityType: "document", entityId: d.id, title: d.title, snippet: d.body.slice(0, 90), score: -0.5 });
  for (const t of store.tasks) if (t.title.toLowerCase().includes(q)) hits.push({ entityType: "task", entityId: t.id, title: t.title, snippet: "task", score: -0.6 });
  for (const c of store.contacts) if ((c.firstName + " " + c.lastName + " " + (c.email ?? "")).toLowerCase().includes(q)) hits.push({ entityType: "contact", entityId: c.id, title: c.firstName + " " + c.lastName, snippet: c.title ?? "contact", score: -0.7 });
  for (const d of store.deals) if (d.name.toLowerCase().includes(q)) hits.push({ entityType: "deal", entityId: d.id, title: d.name, snippet: d.stage, score: -0.8 });
  return { items: hits.slice(0, 8) };
}

function ok(data: unknown) { return { status: 200, data }; }
function created(data: unknown) { return { status: 201, data }; }

export async function demoRequest(method: string, path: string, body?: any): Promise<{ status: number; data: any }> {
  const p = path.split("?")[0];
  const query = new URLSearchParams(path.split("?")[1] ?? "");

  // Auth & session
  if (p === "/auth/me") return ok({ user: store.user, workspaces: [{ ...store.workspace, role: "owner" }] });
  if (p === "/auth/login" || p === "/auth/register") return ok({ token: "demo", user: store.user, workspace: store.workspace });
  if (p === "/auth/logout") return ok({ ok: true });

  // Analytics
  if (p === "/analytics/overview") return ok(overview());
  if (p === "/analytics/pipeline-history") return ok({ items: [] });

  // Search
  if (p === "/search") return ok(search(query.get("q") ?? ""));

  // Workspace
  if (p === "/workspace/members") return ok({ items: store.members });
  if (p === "/workspace/audit") return ok({ items: [...store.audit].reverse() });
  if (p === "/workspace" && method === "PATCH") return ok({ ok: true });

  // API keys
  if (p === "/auth/api-keys" && method === "GET") return ok({ items: store.apiKeys });
  if (p === "/auth/api-keys" && method === "POST") {
    const key = { id: nextId("key"), name: body?.name ?? "demo key", prefix: "bnx_demo0", scopes: body?.scopes ?? ["*"], createdBy: store.user.id, createdAt: new Date().toISOString(), lastUsedAt: null, revokedAt: null };
    store.apiKeys.push(key as any);
    return created({ id: key.id, name: key.name, scopes: key.scopes, token: "bnx_demo_token_shown_once" });
  }

  // CRM
  if (p === "/crm/contacts" && method === "GET") {
    const q = query.get("q")?.toLowerCase();
    const items = q ? store.contacts.filter((c) => (c.firstName + c.lastName + (c.email ?? "")).toLowerCase().includes(q)) : store.contacts;
    return ok({ items, total: items.length, limit: 50, offset: 0 });
  }
  if (p === "/crm/contacts" && method === "POST") {
    const c = { id: nextId("cnt"), firstName: body.firstName, lastName: body.lastName, email: body.email ?? null, phone: body.phone ?? null, title: body.title ?? null, companyId: body.companyId ?? null, notes: body.notes ?? null, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.contacts.unshift(c as any);
    return created(c);
  }
  if (p === "/crm/companies" && method === "GET") return ok({ items: store.companies, total: store.companies.length, limit: 50, offset: 0 });
  if (p === "/crm/companies" && method === "POST") {
    const c = { id: nextId("cmp"), name: body.name, domain: body.domain ?? null, industry: body.industry ?? null, size: body.size ?? null, website: body.website ?? null, notes: body.notes ?? null, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.companies.unshift(c as any);
    return created(c);
  }
  if (p === "/crm/deals" && method === "GET") return ok({ items: store.deals });
  if (p === "/crm/deals" && method === "POST") {
    const d = { id: nextId("del"), name: body.name, companyId: body.companyId ?? null, contactId: body.contactId ?? null, stage: body.stage ?? "lead", value: body.value ?? 0, currency: body.currency ?? "USD", closeDate: body.closeDate ?? null, ownerId: store.user.id, notes: body.notes ?? null, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.deals.unshift(d as any);
    return created(d);
  }
  if (p.startsWith("/crm/deals/") && method === "PATCH") {
    const d = store.deals.find((x) => x.id === p.split("/")[3]);
    if (!d) return { status: 404, data: { error: { code: "not_found", message: "Deal not found" } } };
    Object.assign(d, body ?? {});
    return ok(d);
  }

  // Projects & tasks
  if (p === "/projects" && method === "GET") return ok({ items: store.projects });
  if (p === "/projects" && method === "POST") {
    const proj = { id: nextId("prj"), name: body.name, key: (body.key ?? body.name).toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 6) || "PROJ", description: body.description ?? null, status: body.status ?? "active", color: body.color ?? "#ff6b35", dueDate: body.dueDate ?? null, createdBy: store.user.id, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.projects.unshift(proj as any);
    return created(proj);
  }
  if (p === "/projects/tasks" && method === "GET") {
    const projectId = query.get("projectId");
    const items = projectId ? store.tasks.filter((t) => t.projectId === projectId) : store.tasks;
    return ok({ items });
  }
  if (p === "/projects/tasks" && method === "POST") {
    const t = { id: nextId("tsk"), projectId: body.projectId ?? null, title: body.title, description: body.description ?? null, status: body.status ?? "todo", priority: body.priority ?? "medium", position: store.tasks.length + 1, assigneeId: body.assigneeId ?? null, dueDate: body.dueDate ?? null, messageId: null, createdBy: store.user.id, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.tasks.push(t as any);
    return created(t);
  }
  if (p.startsWith("/projects/tasks/") && p.endsWith("/move") && method === "POST") {
    const t = store.tasks.find((x) => x.id === p.split("/")[3]);
    if (!t) return { status: 404, data: { error: { code: "not_found", message: "Task not found" } } };
    t.status = body.status;
    t.position = body.position ?? t.position;
    return ok(t);
  }

  // Documents
  if (p === "/documents" && method === "GET") return ok({ items: store.documents });
  if (p === "/documents" && method === "POST") {
    const d = { id: nextId("doc"), title: body.title, slug: body.title.toLowerCase().replace(/[^a-z0-9]+/g, "-"), body: body.body ?? "", parentId: body.parentId ?? null, createdBy: store.user.id, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.documents.unshift(d as any);
    return created(d);
  }
  if (p.startsWith("/documents/") && method === "PATCH") {
    const d = store.documents.find((x) => x.id === p.split("/")[2]);
    if (!d) return { status: 404, data: { error: { code: "not_found", message: "Document not found" } } };
    Object.assign(d, body ?? {}, { updatedAt: new Date().toISOString() });
    return ok(d);
  }
  if (p.startsWith("/documents/") && method === "DELETE") {
    store.documents = store.documents.filter((x) => x.id !== p.split("/")[2]);
    return ok({ ok: true });
  }

  // Calendar
  if (p === "/calendar/events" && method === "GET") return ok({ items: store.events });
  if (p === "/calendar/events" && method === "POST") {
    const e = { id: nextId("evt"), title: body.title, description: body.description ?? null, startsAt: body.startsAt, endsAt: body.endsAt, allDay: body.allDay ?? false, location: body.location ?? null, color: body.color ?? "#ff6b35", createdBy: store.user.id, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.events.push(e as any);
    return created(e);
  }

  // Invoices
  if (p === "/invoices" && method === "GET") return ok({ items: store.invoices });
  if (p === "/invoices" && method === "POST") {
    const items = (body.items ?? []).map((i: any, idx: number) => ({ id: nextId("iit"), invoiceId: "pending", description: i.description, quantity: i.quantity, unitPrice: i.unitPrice, amount: i.quantity * i.unitPrice, position: idx }));
    const subtotal = items.reduce((s: number, i: any) => s + i.amount, 0);
    const inv = { id: nextId("inv"), number: "INV-2026-" + String(store.invoices.length + 1).padStart(4, "0"), companyId: body.companyId ?? null, contactId: body.contactId ?? null, status: body.status ?? "draft", issueDate: body.issueDate, dueDate: body.dueDate, currency: body.currency ?? "USD", subtotal, taxRate: body.taxRate ?? 0, total: subtotal * (1 + (body.taxRate ?? 0)), notes: body.notes ?? null, createdBy: store.user.id, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), items };
    store.invoices.unshift(inv as any);
    return created(inv);
  }
  if (p.startsWith("/invoices/") && method === "GET") {
    const inv = store.invoices.find((x) => x.id === p.split("/")[2]);
    return inv ? ok(inv) : { status: 404, data: { error: { code: "not_found", message: "Invoice not found" } } };
  }
  if (p.startsWith("/invoices/") && method === "PATCH") {
    const inv = store.invoices.find((x) => x.id === p.split("/")[2]);
    if (!inv) return { status: 404, data: { error: { code: "not_found", message: "Invoice not found" } } };
    Object.assign(inv, body ?? {});
    return ok(inv);
  }

  // Channels & messages
  if (p === "/channels" && method === "GET") {
    return ok({ items: store.channels.map((c) => ({ ...c, messageCount: store.messages.filter((m) => m.channelId === c.id).length })) });
  }
  if (p === "/channels" && method === "POST") {
    const c = { id: nextId("chn"), kind: body.kind ?? "channel", name: body.name, topic: body.topic ?? null, isPrivate: body.isPrivate ?? false, createdBy: store.user.id, createdAt: new Date().toISOString() };
    store.channels.push(c as any);
    return created(c);
  }
  if (p.startsWith("/channels/") && p.endsWith("/messages") && method === "GET") {
    const channelId = p.split("/")[2];
    return ok({ items: store.messages.filter((m) => m.channelId === channelId && !m.threadId) });
  }
  if (p.startsWith("/channels/") && p.endsWith("/messages") && method === "POST") {
    const channelId = p.split("/")[2];
    const m = { id: nextId("msg"), channelId, threadId: body.threadId ?? null, authorType: "user", authorId: store.user.id, body: body.body, meta: null, createdAt: new Date().toISOString() };
    store.messages.push(m as any);
    return created(m);
  }
  if (p.startsWith("/messages/") && p.endsWith("/task")) {
    const m = store.messages.find((x) => x.id === p.split("/")[2]);
    const t = { id: nextId("tsk"), projectId: null, title: (body?.title ?? m?.body ?? "Task").slice(0, 120), description: m?.body ?? null, status: "todo", priority: "medium", position: store.tasks.length + 1, assigneeId: null, dueDate: null, messageId: m?.id ?? null, createdBy: store.user.id, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.tasks.push(t as any);
    return created({ id: t.id, messageId: m?.id ?? null });
  }

  // Agents & integrations
  if (p === "/agents" && method === "GET") return ok({ items: store.agents });
  if (p === "/agents" && method === "POST") {
    const a = { id: nextId("agt"), name: body.name, kind: body.kind ?? "openclaw", status: "idle", config: body.config ?? {}, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    store.agents.push(a as any);
    return created(a);
  }
  if (p === "/agents/events") return ok({ items: [...store.agentEvents].reverse() });
  if (p === "/integrations/openclaw") {
    return ok({ configured: true, gatewayUrl: "http://127.0.0.1:18789", defaultAgent: "agent:businex:main", hasToken: true });
  }
  if (p === "/integrations/openclaw/status") return ok({ ok: true, status: 200, data: { status: "ready", agents: 3 } });
  if (p === "/integrations/openclaw/sessions") return ok({ ok: true, status: 200, data: { sessions: [{ sessionKey: "agent:businex:main" }, { sessionKey: "agent:businex:ops" }] } });
  if (p === "/integrations/openclaw/message") return ok({ ok: true, status: 200, data: { delivered: true } });
  if (p === "/integrations/open-tag") return ok({ configured: false, daemonUrl: null, defaultAgent: null, hasToken: false });

  // Terminals (demo: no PTY behind the browser)
  if (p === "/terminals" && method === "POST") return created({ id: nextId("trm"), cols: body?.cols ?? 120, rows: body?.rows ?? 32 });

  return ok({ ok: true });
}
