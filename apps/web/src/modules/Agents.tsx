import React, { useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useToast } from "../lib/store";
import { Spinner, EmptyState, Badge, Field, timeAgo } from "../components/ui";
import { IconPlus, IconSend } from "../components/icons";

export function AgentsModule() {
  const { push } = useToast();
  const agents = useData<{ items: any[] }>("/agents");
  const events = useData<{ items: any[] }>("/agents/events");
  const openclaw = useData<{ configured: boolean; gatewayUrl: string | null; defaultAgent: string | null; hasToken: boolean }>("/integrations/openclaw");
  const [gatewayUrl, setGatewayUrl] = useState("");
  const [apiToken, setApiToken] = useState("");
  const [defaultAgent, setDefaultAgent] = useState("");
  const [agentName, setAgentName] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [sessions, setSessions] = useState<any[]>([]);
  const [sessionKey, setSessionKey] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  const saveConfig = async () => {
    try {
      await api.put("/integrations/openclaw", {
        gatewayUrl: gatewayUrl || null,
        apiToken: apiToken || undefined,
        defaultAgent: defaultAgent || null,
      });
      push("OpenClaw integration saved", "success");
      openclaw.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const checkStatus = async () => {
    setBusy(true);
    try {
      const res = await api.get<any>("/integrations/openclaw/status");
      setStatus(res.ok ? "Gateway reachable · " + (res.status ?? 200) : "Error: " + JSON.stringify(res.data).slice(0, 160));
    } catch (e: any) {
      setStatus("Error: " + e.message);
    } finally {
      setBusy(false);
    }
  };

  const loadSessions = async () => {
    setBusy(true);
    try {
      const res = await api.get<any>("/integrations/openclaw/sessions");
      const list = Array.isArray(res.data) ? res.data : (res.data?.sessions ?? []);
      setSessions(list);
      if (list.length && !sessionKey) setSessionKey(list[0].sessionKey ?? list[0].id ?? "");
    } catch (e: any) {
      push("Could not load sessions: " + e.message, "error");
    } finally {
      setBusy(false);
    }
  };

  const sendMessage = async () => {
    if (!sessionKey.trim() || !message.trim()) return;
    setBusy(true);
    try {
      const res = await api.post<any>("/integrations/openclaw/message", { sessionKey, message });
      push(res.ok ? "Message delivered to OpenClaw" : "Gateway error", res.ok ? "success" : "error");
      setMessage("");
    } catch (e: any) {
      push(e.message, "error");
    } finally {
      setBusy(false);
    }
  };

  const createAgent = async () => {
    try {
      await api.post("/agents", { name: agentName, kind: "openclaw" });
      push("Agent teammate added", "success");
      setAgentName("");
      agents.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        {/* OpenClaw connection */}
        <div className="card">
          <div className="flex items-center justify-between">
            <h3 className="font-display text-xl">OpenClaw bridge</h3>
            {openclaw.data?.configured
              ? <Badge tone="sage">connected</Badge>
              : <Badge tone="neutral">not configured</Badge>}
          </div>
          <p className="mt-1 text-xs opacity-55">
            Connect a gateway to run agent sessions, list automations, and let agents operate business records.
          </p>
          <div className="mt-4 flex flex-col gap-3">
            <Field label="Gateway URL">
              <input className="input" placeholder="http://127.0.0.1:18789" value={gatewayUrl || openclaw.data?.gatewayUrl || ""} onChange={(e) => setGatewayUrl(e.target.value)} />
            </Field>
            <Field label="API token">
              <input className="input" type="password" placeholder={openclaw.data?.hasToken ? "•••••• (saved)" : "gateway token"} value={apiToken} onChange={(e) => setApiToken(e.target.value)} />
            </Field>
            <Field label="Default agent (session key or agent id)">
              <input className="input" placeholder="agent:businex:main" value={defaultAgent || openclaw.data?.defaultAgent || ""} onChange={(e) => setDefaultAgent(e.target.value)} />
            </Field>
            <div className="flex gap-2">
              <button className="btn btn-primary" onClick={saveConfig}>Save configuration</button>
              <button className="btn" onClick={checkStatus} disabled={busy}>Test connection</button>
            </div>
            {status && <div className="text-xs opacity-70 font-mono">{status}</div>}
          </div>
        </div>

        {/* Console */}
        <div className="card">
          <div className="flex items-center justify-between">
            <h3 className="font-display text-xl">Agent console</h3>
            <button className="btn btn-ghost text-xs" onClick={loadSessions} disabled={busy}>Load sessions</button>
          </div>
          <p className="mt-1 text-xs opacity-55">Send a message to any OpenClaw session and get the reply in your workspace.</p>
          <div className="mt-4 flex flex-col gap-3">
            <Field label="Session key">
              <input className="input font-mono text-xs" placeholder="agent:businex:dashboard:…" list="session-options" value={sessionKey} onChange={(e) => setSessionKey(e.target.value)} />
              <datalist id="session-options">
                {sessions.map((s) => <option key={s.sessionKey ?? s.id} value={s.sessionKey ?? s.id} />)}
              </datalist>
            </Field>
            <Field label="Message">
              <textarea className="textarea" placeholder="Summarize today's pipeline and flag anything urgent…" value={message} onChange={(e) => setMessage(e.target.value)} />
            </Field>
            <button className="btn btn-primary justify-center" onClick={sendMessage} disabled={busy}>
              <IconSend size={15} /> {busy ? "Working…" : "Send to agent"}
            </button>
          </div>
        </div>

        {/* Teammates */}
        <div className="card">
          <h3 className="font-display text-xl">Agent teammates</h3>
          <p className="mt-1 text-xs opacity-55">Agents that can be mentioned in channels with @name.</p>
          <div className="mt-4 flex gap-2">
            <input className="input" placeholder="Agent name (e.g. ops-bot)" value={agentName} onChange={(e) => setAgentName(e.target.value)} />
            <button className="btn btn-primary" onClick={createAgent}><IconPlus size={15} /> Add</button>
          </div>
          <div className="mt-4 flex flex-col gap-2">
            {agents.loading && <Spinner />}
            {(agents.data?.items ?? []).map((a) => (
              <div key={a.id} className="flex items-center justify-between rounded-xl border px-3 py-2" style={{ borderColor: "var(--panel-border)" }}>
                <div className="flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full" style={{ background: a.status === "working" ? "var(--color-ember-500)" : a.status === "idle" ? "var(--color-sage-500)" : "var(--color-ink-400)" }} />
                  <span className="text-sm font-medium">{a.name}</span>
                  <Badge tone="neutral">{a.kind}</Badge>
                </div>
                <Badge tone={a.status === "working" ? "ember" : "sage"}>{a.status}</Badge>
              </div>
            ))}
            {!agents.loading && (agents.data?.items ?? []).length === 0 && (
              <EmptyState title="No agents yet" hint="Add your first agent teammate." />
            )}
          </div>
        </div>

        {/* Activity */}
        <div className="card">
          <h3 className="font-display text-xl">Agent activity</h3>
          <div className="mt-3 flex flex-col gap-2">
            {(events.data?.items ?? []).slice(0, 12).map((e) => (
              <div key={e.id} className="flex items-center gap-3 text-sm">
                <Badge tone="ember">{e.kind}</Badge>
                <span className="truncate opacity-70">{JSON.stringify(e.payload ?? {}).slice(0, 90)}</span>
                <span className="ml-auto text-xs opacity-45">{timeAgo(e.createdAt)}</span>
              </div>
            ))}
            {!events.loading && (events.data?.items ?? []).length === 0 && (
              <div className="text-sm opacity-50">Agent mentions and dispatches appear here.</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
