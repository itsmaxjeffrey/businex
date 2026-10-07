import * as pty from "node-pty";
import os from "node:os";
import { newId } from "./lib/id";
import { bus } from "./events";

export interface TerminalSession {
  id: string;
  workspaceId: string;
  userId: string | null;
  pty: pty.IPty;
  createdAt: number;
  cols: number;
  rows: number;
}

const sessions = new Map<string, TerminalSession>();

function shellFor(platform: string): string {
  if (platform === "win32") return "powershell.exe";
  return process.env.SHELL || (platform === "darwin" ? "/bin/zsh" : "/bin/bash");
}

export function createTerminal(workspaceId: string, userId: string | null, cols = 120, rows = 32): TerminalSession {
  const id = newId("trm");
  const term = pty.spawn(shellFor(os.platform()), ["-l"], {
    name: "xterm-color",
    cols,
    rows,
    cwd: process.env.HOME || os.homedir(),
    env: { ...process.env, TERM: "xterm-256color", BUSINEX_TERMINAL: "1" } as any,
  });
  const session: TerminalSession = { id, workspaceId, userId, pty: term, createdAt: Date.now(), cols, rows };
  sessions.set(id, session);

  term.onData((data) => {
    bus.publish("terminal.data", workspaceId, { terminalId: id, data });
  });
  term.onExit(({ exitCode }) => {
    bus.publish("terminal.exit", workspaceId, { terminalId: id, exitCode });
    sessions.delete(id);
  });
  return session;
}

export function getTerminal(workspaceId: string, id: string): TerminalSession | undefined {
  const s = sessions.get(id);
  return s && s.workspaceId === workspaceId ? s : undefined;
}

export function listTerminals(workspaceId: string): Array<{ id: string; cols: number; rows: number; createdAt: number }> {
  return [...sessions.values()]
    .filter((s) => s.workspaceId === workspaceId)
    .map((s) => ({ id: s.id, cols: s.cols, rows: s.rows, createdAt: s.createdAt }));
}

export function writeTerminal(workspaceId: string, id: string, data: string): boolean {
  const s = getTerminal(workspaceId, id);
  if (!s) return false;
  s.pty.write(data);
  return true;
}

export function resizeTerminal(workspaceId: string, id: string, cols: number, rows: number): boolean {
  const s = getTerminal(workspaceId, id);
  if (!s) return false;
  s.cols = cols; s.rows = rows;
  try { s.pty.resize(cols, rows); } catch { /* pty may have exited */ }
  return true;
}

export function killTerminal(workspaceId: string, id: string): boolean {
  const s = getTerminal(workspaceId, id);
  if (!s) return false;
  try { s.pty.kill(); } catch { /* already gone */ }
  sessions.delete(id);
  return true;
}

export function killAllTerminals(): void {
  for (const s of sessions.values()) {
    try { s.pty.kill(); } catch { /* ignore */ }
  }
  sessions.clear();
}
