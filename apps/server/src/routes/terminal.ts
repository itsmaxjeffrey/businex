import { param } from "../context";
import { Hono } from "hono";
import { authenticate, auth, requireWorkspace, requireScope, type Env } from "../context";
import { createTerminal, listTerminals, getTerminal, killTerminal } from "../terminal";
import { badRequest, notFound } from "../lib/errors";

export const terminalRoutes = new Hono<Env>();
terminalRoutes.use("*", authenticate, requireWorkspace);

terminalRoutes.get("/", requireScope("terminal:read", "member"), async (c) => {
  return c.json({ items: listTerminals(auth(c).workspaceId) });
});

terminalRoutes.post("/", requireScope("terminal:write", "member"), async (c) => {
  const a = auth(c);
  const body = await c.req.json().catch(() => ({}));
  const cols = Number(body.cols ?? 120), rows = Number(body.rows ?? 32);
  if (!Number.isFinite(cols) || !Number.isFinite(rows) || cols < 2 || rows < 2) throw badRequest("Invalid terminal size");
  const session = createTerminal(a.workspaceId, a.userId, cols, rows);
  return c.json({ id: session.id, cols: session.cols, rows: session.rows }, 201);
});

terminalRoutes.get("/:id", requireScope("terminal:read", "member"), async (c) => {
  const session = getTerminal(auth(c).workspaceId, param(c, "id"));
  if (!session) throw notFound("Terminal not found");
  return c.json({ id: session.id, cols: session.cols, rows: session.rows, createdAt: session.createdAt });
});

terminalRoutes.delete("/:id", requireScope("terminal:write", "member"), async (c) => {
  const ok = killTerminal(auth(c).workspaceId, param(c, "id"));
  if (!ok) throw notFound("Terminal not found");
  return c.json({ ok: true });
});
