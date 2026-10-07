import { WebSocketServer, WebSocket } from "ws";
import type { Server, IncomingMessage } from "node:http";
import { getDb, parseJson } from "./db";
import { verifyToken, hashToken } from "./lib/token";
import { bus, type BusinexEvent } from "./events";
import { config } from "./config";
import { roleAtLeast } from "@businex/shared";
import { writeTerminal, resizeTerminal, killTerminal } from "./terminal";

interface Client {
  ws: WebSocket;
  workspaceId: string;
  userId: string | null;
  terminalIds: Set<string>;
  terminalRead: boolean;
  terminalWrite: boolean;
}

function authenticateUpgrade(req: IncomingMessage): { workspaceId: string; userId: string | null; terminalRead: boolean; terminalWrite: boolean } | null {
  const url = new URL(req.url ?? "/", "http://localhost");
  const token = url.searchParams.get("token")
    ?? parseCookie(req.headers.cookie ?? "")["businex_session"];
  if (!token) return null;
  const db = getDb();

  if (token.startsWith("bnx_")) {
    const row = db.prepare("SELECT workspace_id, revoked_at, scopes FROM api_keys WHERE key_hash = ?").get(hashToken(token)) as any;
    if (!row || row.revoked_at) return null;
    const scopes = parseJson<string[]>(row.scopes, []);
    if (!scopes.includes("*") && !scopes.includes("realtime:read")) return null;
    return { workspaceId: row.workspace_id, userId: null,
      terminalRead: scopes.some(s => ["*", "terminal:*", "terminal:read"].includes(s)),
      terminalWrite: scopes.some(s => ["*", "terminal:*", "terminal:write"].includes(s)) };
  }

  const sessionId = verifyToken(token);
  if (!sessionId) return null;
  const session = db.prepare("SELECT user_id, expires_at FROM sessions WHERE id = ?").get(sessionId) as any;
  if (!session || new Date(session.expires_at).getTime() < Date.now()) return null;
  const workspaceId = url.searchParams.get("workspaceId");
  if (!workspaceId) return null;
  const membership = db.prepare("SELECT role FROM memberships WHERE user_id = ? AND workspace_id = ?").get(session.user_id, workspaceId) as { role: any } | undefined;
  if (!membership) return null;
  return { workspaceId, userId: session.user_id, terminalRead: roleAtLeast(membership.role, "member"), terminalWrite: roleAtLeast(membership.role, "member") };
}

function parseCookie(header: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const part of header.split(";")) {
    const idx = part.indexOf("=");
    if (idx > 0) out[part.slice(0, idx).trim()] = decodeURIComponent(part.slice(idx + 1).trim());
  }
  return out;
}

/**
 * One WebSocket endpoint multiplexes three stream types:
 * - realtime bus events for the workspace ("event" messages)
 * - terminal PTY streams ("terminal.data" / "terminal.exit")
 * - client -> server terminal input/resize/kill
 */
export function attachWebSocket(server: Server): WebSocketServer {
  const wss = new WebSocketServer({ noServer: true });
  const clients = new Set<Client>();

  server.on("upgrade", (req, socket, head) => {
    const url = new URL(req.url ?? "/", "http://localhost");
    if (url.pathname !== "/ws") { socket.destroy(); return; }
    if (req.headers.origin && req.headers.origin !== config.webOrigin) {
      socket.write("HTTP/1.1 403 Forbidden\r\n\r\n"); socket.destroy(); return;
    }
    const auth = authenticateUpgrade(req);
    if (!auth) { socket.write("HTTP/1.1 401 Unauthorized\r\n\r\n"); socket.destroy(); return; }
    wss.handleUpgrade(req, socket, head, (ws) => {
      const client: Client = { ws, workspaceId: auth.workspaceId, userId: auth.userId, terminalIds: new Set(), terminalRead: auth.terminalRead, terminalWrite: auth.terminalWrite };
      clients.add(client);
      ws.on("message", (raw) => {
        let msg: any;
        try { msg = JSON.parse(String(raw)); } catch { return; }
        handleClientMessage(client, msg);
      });
      ws.on("close", () => clients.delete(client));
      ws.send(JSON.stringify({ type: "ready", workspaceId: client.workspaceId }));
    });
  });

  function handleClientMessage(client: Client, msg: any): void {
    if (String(msg.type).startsWith("terminal.")) {
      if (!config.terminalEnabled || !client.terminalRead) return;
      if (!["terminal.watch", "terminal.unwatch"].includes(msg.type) && !client.terminalWrite) return;
    }
    switch (msg.type) {
      case "terminal.watch":
        if (typeof msg.terminalId === "string") client.terminalIds.add(msg.terminalId);
        break;
      case "terminal.unwatch":
        client.terminalIds.delete(msg.terminalId);
        break;
      case "terminal.input":
        writeTerminal(client.workspaceId, msg.terminalId, String(msg.data ?? ""));
        break;
      case "terminal.resize":
        resizeTerminal(client.workspaceId, msg.terminalId, Number(msg.cols ?? 80), Number(msg.rows ?? 24));
        break;
      case "terminal.kill":
        killTerminal(client.workspaceId, msg.terminalId);
        break;
      case "ping":
        client.ws.send(JSON.stringify({ type: "pong", at: Date.now() }));
        break;
    }
  }

  bus.on("event", (event: BusinexEvent) => {
    const serialized = JSON.stringify({ type: "event", event });
    for (const client of clients) {
      if (client.workspaceId !== event.workspaceId) continue;
      if (event.type.startsWith("terminal.")) {
        const payload = event.payload as { terminalId: string };
        if (!client.terminalIds.has(payload.terminalId)) continue;
      }
      if (client.ws.readyState === WebSocket.OPEN) client.ws.send(serialized);
    }
  });

  return wss;
}
