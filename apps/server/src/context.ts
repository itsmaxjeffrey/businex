import type { Context, Next } from "hono";
import { getCookie } from "hono/cookie";
import { getDb, parseJson } from "./db";
import { verifyToken, hashToken } from "./lib/token";
import { unauthorized, forbidden, notFound, badRequest } from "./lib/errors";
import { roleAtLeast, type Role } from "@businex/shared";

export interface AuthContext {
  actorType: "user" | "agent";
  userId: string | null;
  workspaceId: string;
  role: Role;
  scopes: string[] | null; // null = session auth (role-based), array = api key auth
  sessionId: string | null;
  apiKeyId: string | null;
}

export type Env = { Variables: { auth: AuthContext } };

function resolveToken(c: Context): string | null {
  const header = c.req.header("authorization");
  if (header && header.toLowerCase().startsWith("bearer ")) return header.slice(7).trim();
  return getCookie(c, "businex_session") ?? null;
}

/**
 * Resolves authentication from a session cookie/token or a scoped API key.
 * API keys carry the workspace id ("bnx_<workspace prefix>_<secret>").
 */
export async function authenticate(c: Context, next: Next) {
  const token = resolveToken(c);
  if (!token) throw unauthorized();

  const db = getDb();
  if (token.startsWith("bnx_")) {
    const keyHash = hashToken(token);
    const row = db.prepare(`
      SELECT id, workspace_id, scopes, revoked_at FROM api_keys WHERE key_hash = ?
    `).get(keyHash) as any;
    if (!row || row.revoked_at) throw unauthorized("Invalid API key");
    db.prepare("UPDATE api_keys SET last_used_at = ? WHERE id = ?").run(new Date().toISOString(), row.id);
    const scopes = parseJson<string[]>(row.scopes, []);
    c.set("auth", {
      actorType: "agent",
      userId: null,
      workspaceId: row.workspace_id,
      role: "agent",
      scopes,
      sessionId: null,
      apiKeyId: row.id,
    });
    await next();
    return;
  }

  const sessionId = verifyToken(token);
  if (!sessionId) throw unauthorized("Invalid session token");
  const session = db.prepare("SELECT id, user_id, expires_at FROM sessions WHERE id = ?").get(sessionId) as any;
  if (!session || new Date(session.expires_at).getTime() < Date.now()) throw unauthorized("Session expired");

  const wsHeader = c.req.header("x-workspace-id") ?? c.req.query("workspaceId");
  if (!wsHeader) {
    // Session valid but no workspace selected yet (e.g. /auth/me listing workspaces).
    c.set("auth", {
      actorType: "user", userId: session.user_id, workspaceId: "", role: "member",
      scopes: null, sessionId: session.id, apiKeyId: null,
    });
    await next();
    return;
  }

  const membership = db.prepare(`
    SELECT role FROM memberships WHERE user_id = ? AND workspace_id = ?
  `).get(session.user_id, wsHeader) as any;
  if (!membership) throw forbidden("Not a member of this workspace");

  c.set("auth", {
    actorType: "user",
    userId: session.user_id,
    workspaceId: wsHeader,
    role: membership.role as Role,
    scopes: null,
    sessionId: session.id,
    apiKeyId: null,
  });
  await next();
}

/** Requires a workspace-scoped auth context. */
export async function requireWorkspace(c: Context, next: Next) {
  const auth = c.get("auth") as AuthContext | undefined;
  if (!auth || !auth.workspaceId) throw unauthorized("Workspace context required");
  await next();
}

export function requireRole(min: Role) {
  return async (c: Context, next: Next) => {
    const auth = c.get("auth") as AuthContext;
    if (auth.scopes === null && !roleAtLeast(auth.role, min)) {
      throw forbidden("Requires role: " + min);
    }
    await next();
  };
}

/** For API-key callers: require a scope like "crm:write". Session callers pass role checks instead. */
export function requireScope(scope: string, minRole: Role = "member") {
  return async (c: Context, next: Next) => {
    const auth = c.get("auth") as AuthContext;
    if (auth.scopes !== null) {
      const [module, verb] = scope.split(":");
      const ok = auth.scopes.some((s) => s === scope || s === module + ":*" || s === "*");
      if (!ok) throw forbidden("API key missing scope: " + scope);
    } else if (!roleAtLeast(auth.role, minRole)) {
      throw forbidden("Requires role: " + minRole);
    }
    await next();
  };
}

/** Route params are strings in practice; assert presence once here. */
export function param(c: Context, name: string): string {
  const value = c.req.param(name);
  if (!value) throw badRequest("Missing route parameter: " + name);
  return value;
}

export function auth(c: Context): AuthContext {
  return c.get("auth") as AuthContext;
}

export function actor(c: Context): { type: "user" | "agent" | "system"; id: string | null } {
  const a = auth(c);
  return { type: a.actorType, id: a.userId ?? a.apiKeyId };
}
