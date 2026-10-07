import { getDb, now, parseJson } from "../db";

export interface OpenTagConfig {
  daemonUrl: string | null;
  apiToken: string | null;
  defaultAgent: string | null;
  enabled: boolean;
}

export function getOpenTagConfig(workspaceId: string): OpenTagConfig {
  const row = getDb().prepare("SELECT config FROM integration_settings WHERE workspace_id = ? AND provider = 'open-tag'")
    .get(workspaceId) as any;
  const cfg = parseJson<Partial<OpenTagConfig>>(row?.config, {});
  return {
    daemonUrl: cfg.daemonUrl ?? null,
    apiToken: cfg.apiToken ?? null,
    defaultAgent: cfg.defaultAgent ?? null,
    enabled: Boolean(cfg.daemonUrl),
  };
}

export function saveOpenTagConfig(workspaceId: string, config: Partial<OpenTagConfig>): void {
  const db = getDb();
  const existing = db.prepare("SELECT id, config FROM integration_settings WHERE workspace_id = ? AND provider = 'open-tag'")
    .get(workspaceId) as any;
  const merged = { ...parseJson<Record<string, unknown>>(existing?.config, {}), ...config };
  if (existing) {
    db.prepare("UPDATE integration_settings SET config = ?, updated_at = ? WHERE id = ?")
      .run(JSON.stringify(merged), now(), existing.id);
  } else {
    db.prepare(`
      INSERT INTO integration_settings (id, workspace_id, provider, config, updated_at) VALUES (?, ?, 'open-tag', ?, ?)
    `).run("int_" + Math.random().toString(36).slice(2, 10), workspaceId, JSON.stringify(merged), now());
  }
}

/**
 * Bridge to an open-tag daemon (https://github.com/fancyboi999/open-tag).
 *
 * Businex stays the business system of record: channel messages, tasks and
 * agent status live in Businex, while the daemon runs the actual agent
 * runtimes (Claude Code, Codex, Copilot, OpenClaw, ...). Agent mentions are
 * dispatched as open-tag work items and the daemon's reply is posted back to
 * the originating Businex thread.
 */
export class OpenTagClient {
  constructor(private cfg: OpenTagConfig) {}

  private async call(path: string, init?: RequestInit): Promise<{ ok: boolean; status: number; data: any }> {
    if (!this.cfg.daemonUrl) {
      return { ok: false, status: 0, data: { error: "open-tag bridge is not configured" } };
    }
    try {
      const res = await fetch(this.cfg.daemonUrl.replace(/\/$/, "") + path, {
        ...init,
        headers: {
          "Content-Type": "application/json",
          ...(this.cfg.apiToken ? { Authorization: "Bearer " + this.cfg.apiToken } : {}),
          ...(init?.headers ?? {}),
        },
        signal: AbortSignal.timeout(15000),
      });
      const text = await res.text();
      let data: any = null;
      try { data = text ? JSON.parse(text) : null; } catch { data = { raw: text.slice(0, 2000) }; }
      return { ok: res.ok, status: res.status, data };
    } catch (e: any) {
      return { ok: false, status: 0, data: { error: e?.message ?? "open-tag daemon unreachable" } };
    }
  }

  status() { return this.call("/api/status"); }
  agents() { return this.call("/api/agents"); }

  /** Dispatch a mention as work for an open-tag managed agent. */
  dispatch(agentName: string, task: string, context: { channelId: string; messageId: string; threadId: string | null }) {
    return this.call("/api/agents/" + encodeURIComponent(agentName) + "/dispatch", {
      method: "POST",
      body: JSON.stringify({ task, context, source: "businex" }),
    });
  }
}
