import React, { useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useSession, useToast } from "../lib/store";
import { Spinner, EmptyState, Badge, Field, formatDate, timeAgo, Avatar } from "../components/ui";
import { IconPlus } from "../components/icons";

export function SettingsModule() {
  const { workspace, user, refresh } = useSession();
  const { push } = useToast();
  const members = useData<{ items: any[] }>("/workspace/members");
  const keys = useData<{ items: any[] }>("/auth/api-keys");
  const audit = useData<{ items: any[] }>("/workspace/audit");
  const [wsName, setWsName] = useState(workspace?.name ?? "");
  const [keyName, setKeyName] = useState("");
  const [newKey, setNewKey] = useState<string | null>(null);

  const saveWorkspace = async () => {
    try {
      await api.patch("/workspace", { name: wsName });
      push("Workspace updated", "success");
      void refresh();
    } catch (e: any) { push(e.message, "error"); }
  };

  const createKey = async () => {
    try {
      const res = await api.post<any>("/auth/api-keys", { name: keyName, scopes: ["*"] });
      setNewKey(res.token);
      setKeyName("");
      keys.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const revokeKey = async (id: string) => {
    try {
      await api.del("/auth/api-keys/" + id);
      push("API key revoked", "success");
      keys.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  return (
    <div className="h-full overflow-y-auto p-6">
      <h2 className="font-display text-2xl">Workspace settings</h2>

      <div className="mt-5 grid grid-cols-1 gap-5 lg:grid-cols-2">
        <div className="card">
          <h3 className="font-display text-lg">General</h3>
          <div className="mt-4 flex flex-col gap-3">
            <Field label="Workspace name">
              <input className="input" value={wsName} onChange={(e) => setWsName(e.target.value)} />
            </Field>
            <div className="flex gap-2">
              <button className="btn btn-primary" onClick={saveWorkspace}>Save changes</button>
            </div>
            <div className="mt-2 text-xs opacity-50">
              Signed in as {user?.email} · role {workspace?.role}
            </div>
          </div>
        </div>

        <div className="card">
          <h3 className="font-display text-lg">API keys</h3>
          <p className="mt-1 text-xs opacity-55">Scoped keys for agents and integrations (Bearer bnx_…).</p>
          <div className="mt-3 flex gap-2">
            <input className="input" placeholder="Key name (e.g. openclaw-agent)" value={keyName} onChange={(e) => setKeyName(e.target.value)} />
            <button className="btn btn-primary" onClick={createKey}><IconPlus size={14} /> Create</button>
          </div>
          {newKey && (
            <div className="mt-3 rounded-xl border p-3 text-xs font-mono" style={{ borderColor: "rgba(255,107,53,0.4)", background: "rgba(255,107,53,0.08)" }}>
              Copy now — shown once: <span className="break-all">{newKey}</span>
            </div>
          )}
          <div className="mt-4 flex flex-col gap-2">
            {keys.loading && <Spinner />}
            {(keys.data?.items ?? []).map((k) => (
              <div key={k.id} className="flex items-center justify-between rounded-xl border px-3 py-2" style={{ borderColor: "var(--panel-border)" }}>
                <div>
                  <div className="text-sm font-medium">{k.name}</div>
                  <div className="font-mono text-[11px] opacity-50">{k.prefix}… · {k.revokedAt ? "revoked" : k.lastUsedAt ? "used " + timeAgo(k.lastUsedAt) : "never used"}</div>
                </div>
                {!k.revokedAt && <button className="btn btn-danger text-xs" onClick={() => revokeKey(k.id)}>Revoke</button>}
              </div>
            ))}
            {!keys.loading && (keys.data?.items ?? []).length === 0 && <EmptyState title="No API keys" hint="Create one to connect agents." />}
          </div>
        </div>

        <div className="card">
          <h3 className="font-display text-lg">Members</h3>
          <div className="mt-3 flex flex-col gap-2">
            {members.loading && <Spinner />}
            {(members.data?.items ?? []).map((m) => (
              <div key={m.id} className="flex items-center gap-3 rounded-xl border px-3 py-2" style={{ borderColor: "var(--panel-border)" }}>
                <Avatar name={m.user.name} color={m.user.avatarColor} size={28} />
                <div className="flex-1">
                  <div className="text-sm font-medium">{m.user.name}</div>
                  <div className="text-[11px] opacity-50">{m.user.email}</div>
                </div>
                <Badge tone={m.role === "owner" ? "ember" : "neutral"}>{m.role}</Badge>
              </div>
            ))}
          </div>
        </div>

        <div className="card">
          <h3 className="font-display text-lg">Audit log</h3>
          <p className="mt-1 text-xs opacity-55">Every mutation is recorded with actor and timestamp.</p>
          <div className="mt-3 flex flex-col gap-1.5">
            {audit.loading && <Spinner />}
            {(audit.data?.items ?? []).slice(0, 20).map((a) => (
              <div key={a.id} className="flex items-center gap-3 text-xs">
                <Badge tone={a.actorType === "agent" ? "ember" : "neutral"}>{a.actorType}</Badge>
                <span className="font-mono">{a.action}</span>
                <span className="opacity-55">{a.entityType}</span>
                <span className="ml-auto opacity-45">{formatDate(a.createdAt)}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
