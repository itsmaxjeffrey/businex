import { beforeAll, describe, expect, it } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";

let app: { request: (path: string, init?: RequestInit) => Promise<Response> };
let token = "";
let ws = "";
const dataDirs: string[] = [];

beforeAll(async () => {
  const dataDir = mkdtempSync(path.join(os.tmpdir(), "businex-test-"));
  dataDirs.push(dataDir);
  process.env.BUSINEX_DATA_DIR = dataDir;
  process.env.NODE_ENV = "test";
  const mod = await import("../src/app");
  app = mod.createApp();
});

process.on("exit", () => {
  for (const dir of dataDirs) {
    try { rmSync(dir, { recursive: true, force: true }); } catch { /* ignore */ }
  }
});

async function call(method: string, apiPath: string, body?: unknown, headers: Record<string, string> = {}) {
  const res = await app.request("/api" + apiPath, {
    method,
    headers: {
      "Content-Type": "application/json",
      Authorization: "Bearer " + token,
      "X-Workspace-Id": ws,
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await res.json().catch(() => null);
  return { status: res.status, data };
}

describe("core", () => {
  it("reports health", async () => {
    const res = await call("GET", "/health");
    expect(res.status).toBe(200);
    expect(res.data.ok).toBe(true);
  });

  it("registers a user with a clean workspace", async () => {
    const res = await call("POST", "/auth/register", {
      email: "test@businex.local", name: "Test User", password: "test-pass-1", workspaceName: "Test Co",
    });
    expect(res.status).toBe(201);
    expect(res.data.token).toBeTruthy();
    expect(res.data.workspace.id).toBeTruthy();
    token = res.data.token;
    ws = res.data.workspace.id;
  });

  it("returns the session user and workspaces", async () => {
    const res = await call("GET", "/auth/me");
    expect(res.status).toBe(200);
    expect(res.data.user.email).toBe("test@businex.local");
    expect(res.data.workspaces[0].role).toBe("owner");
  });

  it("rejects invalid login", async () => {
    const res = await call("POST", "/auth/login", { email: "test@businex.local", password: "wrong" });
    expect(res.status).toBe(401);
  });

  it("rejects tampered session tokens", async () => {
    const res = await call("GET", "/auth/me", undefined, { Authorization: "Bearer ses_forged.signature" });
    expect(res.status).toBe(401);
  });
});

describe("crm", () => {
  let contactId = "";

  it("creates a contact", async () => {
    const res = await call("POST", "/crm/contacts", {
      firstName: "Anna", lastName: "Keller", email: "anna@acme.example", title: "CTO",
    });
    expect(res.status).toBe(201);
    contactId = res.data.id;
  });

  it("lists and updates the contact", async () => {
    const list = await call("GET", "/crm/contacts");
    expect(list.data.items).toHaveLength(1);

    const upd = await call("PATCH", "/crm/contacts/" + contactId, { title: "VP Engineering" });
    expect(upd.status).toBe(200);
    expect(upd.data.title).toBe("VP Engineering");
  });

  it("finds the contact via full-text search", async () => {
    const res = await call("GET", "/search?q=anna");
    const hit = res.data.items.find((h: any) => h.entityId === contactId);
    expect(hit).toBeTruthy();
  });

  it("computes invoice totals with tax", async () => {
    const res = await call("POST", "/invoices", {
      issueDate: "2026-10-07", dueDate: "2026-11-07", taxRate: 0.1,
      items: [{ description: "Consulting", quantity: 2, unitPrice: 500 }],
    });
    expect(res.status).toBe(201);
    expect(res.data.subtotal).toBe(1000);
    expect(res.data.total).toBe(1100);
  });

  it("moves tasks through the pipeline", async () => {
    const task = await call("POST", "/projects/tasks", { title: "Write docs", priority: "high" });
    expect(task.status).toBe(201);

    const moved = await call("POST", "/projects/tasks/" + task.data.id + "/move", { status: "done", position: 1 });
    expect(moved.data.status).toBe("done");
  });

  it("converts channel messages into tasks", async () => {
    const channels = await call("GET", "/channels");
    const general = channels.data.items.find((c: any) => c.name === "general");
    expect(general).toBeTruthy();

    const msg = await call("POST", "/channels/" + general.id + "/messages", { body: "We should track this" });
    expect(msg.status).toBe(201);

    const task = await call("POST", "/messages/" + msg.data.id + "/task", { title: "Track it" });
    expect(task.status).toBe(201);
    expect(task.data.messageId).toBe(msg.data.id);
  });
});

describe("workspace isolation and scopes", () => {
  it("cannot see another workspace's records", async () => {
    const ownToken = token;
    const ownWs = ws;

    const reg = await call("POST", "/auth/register", {
      email: "other@businex.local", name: "Other", password: "other-pass-1",
    });
    expect(reg.status).toBe(201);

    const list = await app.request("/api/crm/contacts", {
      headers: {
        Authorization: "Bearer " + reg.data.token,
        "X-Workspace-Id": reg.data.workspace.id,
      },
    });
    const body = await list.json();
    expect(list.status).toBe(200);
    expect(body.items).toHaveLength(0);

    const crossWorkspace = await app.request("/api/crm/contacts", {
      headers: {
        Authorization: "Bearer " + reg.data.token,
        "X-Workspace-Id": ownWs,
      },
    });
    expect(crossWorkspace.status).toBe(403);

    token = ownToken;
    ws = ownWs;
  });

  it("enforces API key scopes", async () => {
    const keyRes = await call("POST", "/auth/api-keys", { name: "reader", scopes: ["crm:read"] });
    expect(keyRes.status).toBe(201);
    const keyToken = keyRes.data.token;

    const read = await app.request("/api/crm/contacts", {
      headers: { Authorization: "Bearer " + keyToken },
    });
    expect(read.status).toBe(200);

    const write = await app.request("/api/projects/tasks", {
      method: "POST",
      headers: { Authorization: "Bearer " + keyToken, "Content-Type": "application/json" },
      body: JSON.stringify({ title: "denied" }),
    });
    expect(write.status).toBe(403);
  });

  it("audits mutations", async () => {
    const res = await call("GET", "/workspace/audit");
    expect(res.status).toBe(200);
    expect(res.data.items.length).toBeGreaterThan(3);
  });
});


describe("deployment boundaries", () => {
  it("never returns password recovery credentials or discloses whether an account exists", async () => {
    const known = await call("POST", "/auth/forgot-password", { email: "test@businex.local" });
    const unknown = await call("POST", "/auth/forgot-password", { email: "missing@businex.local" });
    expect(known).toEqual(unknown);
    expect(known.data).toEqual({ ok: true });
  });
  it("rejects cross-origin mutations before changing business data", async () => {
    const res = await call("POST", "/crm/contacts", { firstName: "Rejected" }, { Origin: "https://untrusted.example" });
    expect(res.status).toBe(403);
    const list = await call("GET", "/crm/contacts");
    expect(list.data.items.some((x: any) => x.firstName === "Rejected")).toBe(false);
  });
  it("does not advertise credentialed CORS to unrelated websites", async () => {
    const res = await app.request("/api/health", { headers: { Origin: "https://untrusted.example" } });
    expect(res.headers.get("access-control-allow-origin")).toBeNull();
  });
  it("allows the configured browser origin", async () => {
    const res = await app.request("/api/health", { headers: { Origin: "http://localhost:5199" } });
    expect(res.headers.get("access-control-allow-origin")).toBe("http://localhost:5199");
  });
});


describe("project and task workflows", () => {
  it("lists created tasks through the task collection endpoint", async () => {
    const task = await call("POST", "/projects/tasks", { title: "Visible in task list" });
    const list = await call("GET", "/projects/tasks");
    expect(list.status).toBe(200);
    expect(list.data.items.some((x: any) => x.id === task.data.id)).toBe(true);
  });
  it("updates and clears project due dates without breaking detail retrieval", async () => {
    const project = await call("POST", "/projects", { name: "Deployment", dueDate: "2026-11-01" });
    const changed = await call("PATCH", "/projects/" + project.data.id, { dueDate: "2026-11-10" });
    expect(changed.data.dueDate).toBe("2026-11-10");
    const cleared = await call("PATCH", "/projects/" + project.data.id, { dueDate: null });
    expect(cleared.data.dueDate).toBeNull();
    expect((await call("GET", "/projects/" + project.data.id)).data.id).toBe(project.data.id);
  });
  it("isolates task comments and project assignment across workspaces", async () => {
    const project = await call("POST", "/projects", { name: "Private project" });
    const task = await call("POST", "/projects/tasks", { title: "Private task", projectId: project.data.id });
    expect((await call("POST", "/projects/tasks/" + task.data.id + "/comments", { body: "Private comment" })).status).toBe(201);
    const other = await call("POST", "/auth/register", { email: "comments-other@businex.local", name: "Other workspace", password: "other-test-pass" });
    const headers = { Authorization: "Bearer " + other.data.token, "X-Workspace-Id": other.data.workspace.id };
    expect((await call("GET", "/projects/tasks/" + task.data.id + "/comments", undefined, headers)).status).toBe(404);
    expect((await call("POST", "/projects/tasks/" + task.data.id + "/comments", { body: "Blocked" }, headers)).status).toBe(404);
    expect((await call("POST", "/projects/tasks", { title: "Blocked", projectId: project.data.id }, headers)).status).toBe(404);
  });
});
