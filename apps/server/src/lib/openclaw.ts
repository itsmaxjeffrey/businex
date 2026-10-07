import { getDb, now, parseJson } from "../db";

export interface OpenClawConfig {
  gatewayUrl: string | null;
  apiToken: string | null;
  defaultAgent: string | null;
  enabled: boolean;
}

export function getOpenClawConfig(workspaceId: string): OpenClawConfig {
  const row = getDb().prepare("SELECT config FROM integration_settings WHERE workspace_id = ? AND provider = 'openclaw'")
    .get(workspaceId) as any;
  const cfg = parseJson<Partial<OpenClawConfig>>(row?.config, {});
  return {
    gatewayUrl: cfg.gatewayUrl ?? null,
    apiToken: cfg.apiToken ?? null,
    defaultAgent: cfg.defaultAgent ?? null,
    enabled: Boolean(cfg.gatewayUrl && cfg.apiToken),
  };
}

export function saveOpenClawConfig(workspaceId: string, config: Partial<OpenClawConfig>): void {
  const db = getDb();
  const existing = db.prepare("SELECT id, config FROM integration_settings WHERE workspace_id = ? AND provider = 'openclaw'")
    .get(workspaceId) as any;
  const merged = { ...parseJson<Record<string, unknown>>(existing?.config, {}), ...config };
  if (existing) {
    db.prepare("UPDATE integration_settings SET config = ?, updated_at = ? WHERE id = ?")
      .run(JSON.stringify(merged), now(), existing.id);
  } else {
    db.prepare(`
      INSERT INTO integration_settings (id, workspace_id, provider, config, updated_at) VALUES (?, ?, 'openclaw', ?, ?)
    `).run("int_" + Math.random().toString(36).slice(2, 10), workspaceId, JSON.stringify(merged), now());
  }
}

/**
 * Thin client for an OpenClaw gateway. Uses the gateway's HTTP surface:
 * agents/sessions listing and message delivery. Failures are returned as
 * structured results so the UI can surface them without breaking the module.
 */
export class OpenClawClient {
  constructor(private cfg: OpenClawConfig) {}

  private async call(path: string, init?: RequestInit): Promise<{ ok: boolean; status: number; data: any }> {
    if (!this.cfg.gatewayUrl || !this.cfg.apiToken) {
      return { ok: false, status: 0, data: { error: "OpenClaw integration is not configured" } };
    }
    try {
      const res = await fetch(this.cfg.gatewayUrl.replace(/\/$/, "") + path, {
        ...init,
        headers: {
          "Content-Type": "application/json",
          Authorization: "Bearer " + this.cfg.apiToken,
          ...(init?.headers ?? {}),
        },
        signal: AbortSignal.timeout(15000),
      });
      const text = await res.text();
      let data: any = null;
      try { data = text ? JSON.parse(text) : null; } catch { data = { raw: text.slice(0, 2000) }; }
      return { ok: res.ok, status: res.status, data };
    } catch (e: any) {
      return { ok: false, status: 0, data: { error: e?.message ?? "OpenClaw gateway unreachable" } };
    }
  }

  status() { return this.call("/api/status"); }
  agents() { return this.call("/api/agents"); }
  sessions() { return this.call("/api/sessions"); }
  automations() { return this.call("/api/automations"); }
  sendMessage(sessionKey: string, message: string) {
    return this.call("/api/sessions/" + encodeURIComponent(sessionKey) + "/message", {
      method: "POST",
      body: JSON.stringify({ message }),
    });
  }
}
