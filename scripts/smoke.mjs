#!/usr/bin/env node
// End-to-end smoke test: boots a clean server on a private port + data dir,
// exercises every module through the real HTTP API, then shuts down.
//
// Uses node:http directly so local checks work even when the environment
// injects HTTP(S) proxy variables (node fetch would route through them).
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Some environments inject HTTP(S) proxy variables (NODE_USE_ENV_PROXY=1 makes
// even node:http honor them). This test only talks to its own localhost server,
// so scrub proxy configuration before the first request.
process.env.NO_PROXY = "127.0.0.1,localhost";
process.env.no_proxy = "127.0.0.1,localhost";
delete process.env.HTTP_PROXY;
delete process.env.HTTPS_PROXY;
delete process.env.http_proxy;
delete process.env.https_proxy;
delete process.env.ALL_PROXY;
delete process.env.all_proxy;
delete process.env.NODE_USE_ENV_PROXY;

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PORT = process.env.BUSINEX_SMOKE_PORT ? Number(process.env.BUSINEX_SMOKE_PORT) : await new Promise((resolve, reject) => {
  const probe = net.createServer();
  probe.once("error", reject);
  probe.listen(0, "127.0.0.1", () => {
    const port = probe.address().port;
    probe.close(() => resolve(port));
  });
});
const BASE = "http://127.0.0.1:" + PORT;
const dataDir = mkdtempSync(path.join(os.tmpdir(), "businex-smoke-"));

/** Strip proxy configuration so local requests never leave the machine. */
function cleanEnv(extra = {}) {
  const env = { ...process.env, ...extra };
  delete env.HTTP_PROXY;
  delete env.HTTPS_PROXY;
  delete env.http_proxy;
  delete env.https_proxy;
  delete env.ALL_PROXY;
  delete env.all_proxy;
  delete env.NODE_USE_ENV_PROXY;
  env.NO_PROXY = "127.0.0.1,localhost";
  env.no_proxy = "127.0.0.1,localhost";
  return env;
}

/** Minimal HTTP/1.1 client over raw TCP — immune to any proxy injection. */
function dechunk(raw) {
  let out = "";
  let rest = raw;
  while (rest.length > 0) {
    const idx = rest.indexOf("\r\n");
    if (idx < 0) break;
    const size = parseInt(rest.slice(0, idx), 16);
    if (!Number.isFinite(size) || size <= 0) break;
    out += rest.slice(idx + 2, idx + 2 + size);
    rest = rest.slice(idx + 2 + size + 2);
  }
  return out;
}

function request(method, url, body, headers = {}) {
  return new Promise((resolve, reject) => {
    const parsed = new URL(url);
    const payload = body === undefined ? null : JSON.stringify(body);
    const lines = [
      method + " " + parsed.pathname + parsed.search + " HTTP/1.1",
      "Host: " + parsed.host,
      "Connection: close",
    ];
    for (const [k, v] of Object.entries(headers)) lines.push(k + ": " + v);
    if (payload) {
      lines.push("Content-Type: application/json");
      lines.push("Content-Length: " + Buffer.byteLength(payload));
    }

    const sock = net.connect(Number(parsed.port), parsed.hostname, () => {
      sock.write(lines.join("\r\n") + "\r\n\r\n" + (payload ?? ""));
    });
    sock.setTimeout(15000, () => { sock.destroy(); reject(new Error("request timeout: " + url)); });

    let data = "";
    sock.setEncoding("utf8");
    sock.on("data", (chunk) => { data += chunk; });
    sock.on("end", () => {
      const idx = data.indexOf("\r\n\r\n");
      const head = idx >= 0 ? data.slice(0, idx) : data;
      let rawBody = idx >= 0 ? data.slice(idx + 4) : "";
      if (/transfer-encoding:\s*chunked/i.test(head)) rawBody = dechunk(rawBody);
      const status = Number(head.split(" ")[1] ?? 0);
      let parsedBody = null;
      try { parsedBody = rawBody ? JSON.parse(rawBody) : null; } catch { parsedBody = rawBody; }
      resolve({ status, data: parsedBody });
    });
    sock.on("error", reject);
  });
}

let token = "";
let ws = "";

async function call(method, apiPath, body, headers = {}) {
  return request(method, BASE + "/api" + apiPath, body, {
    Authorization: "Bearer " + token,
    "X-Workspace-Id": ws,
    ...headers,
  });
}

let passed = 0;
let failed = 0;

function check(name, condition, detail) {
  if (condition) {
    passed++;
    console.log("  \u001b[32m\u2713\u001b[0m " + name);
  } else {
    failed++;
    console.log("  \u001b[31m\u2717\u001b[0m " + name + (detail ? " \u2014 " + JSON.stringify(detail).slice(0, 220) : ""));
  }
}

const server = spawn(process.execPath, ["--import", "tsx", "src/index.ts"], {
  cwd: path.join(root, "apps/server"),
  env: cleanEnv({ BUSINEX_PORT: String(PORT), BUSINEX_DATA_DIR: dataDir, NODE_ENV: "test" }),
  stdio: ["ignore", "pipe", "pipe"],
});

let serverLog = "";
server.stdout.on("data", (c) => { serverLog += c.toString(); });
server.stderr.on("data", (c) => { serverLog += c.toString(); });

async function waitForServer() {
  for (let i = 0; i < 80; i++) {
    try {
      const res = await request("GET", BASE + "/api/health");
      if (res.status === 200) return true;
    } catch { /* not up yet */ }
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}

async function cleanup(code) {
  if (server.exitCode === null && server.signalCode === null) {
    await new Promise((resolve) => {
      const timer = setTimeout(() => { server.kill("SIGKILL"); }, 3000);
      server.once("exit", () => { clearTimeout(timer); resolve(); });
      server.kill("SIGTERM");
    });
  }
  rmSync(dataDir, { recursive: true, force: true });
  process.exit(code);
}

async function main() {
  console.log("Businex smoke test \u2014 port " + PORT + ", data " + dataDir);
  const up = await waitForServer();
  if (!up) {
    console.error("Server did not start. Log:\n" + serverLog.slice(-2000));
    await cleanup(1);
    return;
  }

  console.log("\nCore");
  const health = await call("GET", "/health");
  check("health endpoint", health.status === 200 && health.data?.ok === true, health);

  const reg = await call("POST", "/auth/register", {
    email: "smoke@businex.local", name: "Smoke Test", password: "smoke-test-1", workspaceName: "Smoke Co",
  });
  check("register creates user + workspace", reg.status === 201 && Boolean(reg.data?.token), reg);
  token = reg.data?.token ?? "";
  ws = reg.data?.workspace?.id ?? "";

  const me = await call("GET", "/auth/me");
  check("session works", me.status === 200 && me.data?.user?.email === "smoke@businex.local", me);

  console.log("\nCRM");
  const contact = await call("POST", "/crm/contacts", { firstName: "Anna", lastName: "Keller", email: "anna@acme.example" });
  check("create contact", contact.status === 201, contact);
  const company = await call("POST", "/crm/companies", { name: "Acme", domain: "acme.example" });
  check("create company", company.status === 201, company);
  const deal = await call("POST", "/crm/deals", { name: "Big deal", value: 12000, stage: "lead" });
  check("create deal", deal.status === 201, deal);
  const dealWon = await call("PATCH", "/crm/deals/" + (deal.data?.id ?? "missing"), { stage: "won" });
  check("move deal to won", dealWon.status === 200 && dealWon.data?.stage === "won", dealWon);

  console.log("\nProjects");
  const project = await call("POST", "/projects", { name: "Website Redesign" });
  check("create project", project.status === 201, project);
  const task = await call("POST", "/projects/tasks", { title: "Draft hero copy", projectId: project.data?.id });
  check("create task", task.status === 201, task);
  const taskList = await call("GET", "/projects/tasks");
  check("created task appears in task list", taskList.status === 200 && (taskList.data?.items ?? []).some(t => t.id === task.data?.id), taskList);
  const moved = await call("POST", "/projects/tasks/" + (task.data?.id ?? "missing") + "/move", { status: "done", position: 5 });
  check("move task to done", moved.status === 200 && moved.data?.status === "done", moved);

  console.log("\nDocuments & search");
  const doc = await call("POST", "/documents", { title: "Onboarding playbook", body: "How we onboard customers quickly." });
  check("create document", doc.status === 201, doc);
  const search = await call("GET", "/search?q=onboarding");
  const hit = (search.data?.items ?? []).find((h) => h.entityType === "document");
  check("full-text search finds document", Boolean(hit), search);

  console.log("\nCalendar & invoices");
  const starts = new Date(Date.now() + 86400000).toISOString();
  const evt = await call("POST", "/calendar/events", { title: "Board meeting", startsAt: starts, endsAt: new Date(Date.now() + 90000000).toISOString() });
  check("create event", evt.status === 201, evt);
  const invoice = await call("POST", "/invoices", {
    issueDate: "2026-10-07", dueDate: "2026-11-07", taxRate: 0.1,
    items: [{ description: "Consulting", quantity: 2, unitPrice: 500 }],
  });
  check("create invoice with totals", invoice.status === 201 && invoice.data?.total === 1100, invoice);

  console.log("\nChannels (open-tag model)");
  const channels = await call("GET", "/channels");
  const general = (channels.data?.items ?? []).find((c) => c.name === "general");
  check("general channel seeded", Boolean(general), channels);
  const msg = await call("POST", "/channels/" + (general?.id ?? "missing") + "/messages", { body: "Hello team" });
  check("post channel message", msg.status === 201, msg);
  const fromMsg = await call("POST", "/messages/" + (msg.data?.id ?? "missing") + "/task", { title: "Follow up on hello" });
  check("convert message to task", fromMsg.status === 201, fromMsg);

  console.log("\nTerminal (PTY)");
  const term = await call("POST", "/terminals", { cols: 80, rows: 24 });
  check("spawn PTY terminal", term.status === 201, term);
  const killed = await call("DELETE", "/terminals/" + (term.data?.id ?? "missing"));
  check("kill PTY terminal", killed.status === 200, killed);

  console.log("\nAgents & MCP");
  const agent = await call("POST", "/agents", { name: "ops-bot", kind: "openclaw" });
  check("add agent teammate", agent.status === 201, agent);
  const keyRes = await call("POST", "/auth/api-keys", { name: "smoke-agent", scopes: ["*"] });
  check("create scoped API key", keyRes.status === 201 && Boolean(keyRes.data?.token), keyRes);
  const mcp = await request("POST", BASE + "/api/mcp", {
    jsonrpc: "2.0", id: 1, method: "tools/call",
    params: { name: "businex.task.create", arguments: { title: "Agent task" } },
  }, { Authorization: "Bearer " + (keyRes.data?.token ?? "") });
  check("MCP tool call creates task", mcp.status === 200 && (mcp.data?.result?.content?.length ?? 0) > 0, mcp);

  const readKeyRes = await call("POST", "/auth/api-keys", { name: "read-only", scopes: ["crm:read"] });
  const denied = await request("POST", BASE + "/api/projects/tasks", { title: "should be denied" }, {
    Authorization: "Bearer " + (readKeyRes.data?.token ?? ""),
  });
  check("scope enforcement denies write", denied.status === 403, { status: denied.status });

  console.log("\nAudit");
  const audit = await call("GET", "/workspace/audit");
  check("audit log recorded mutations", (audit.data?.items ?? []).length >= 5, audit);

  console.log("\n\u001b[1m" + passed + " passed, " + failed + " failed\u001b[0m");
  await cleanup(failed > 0 ? 1 : 0);
}

main().catch(async (err) => {
  console.error("Smoke test crashed:", err);
  await cleanup(1);
});
