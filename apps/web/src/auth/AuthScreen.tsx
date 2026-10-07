import React, { useState } from "react";
import { useSession } from "../lib/store";

export function AuthScreen() {
  const { login, register } = useSession();
  const [mode, setMode] = useState<"login" | "register">("login");
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [workspaceName, setWorkspaceName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (mode === "login") {
        await login(email, password);
      } else {
        await register(email, name, password, workspaceName || undefined);
      }
    } catch (err: any) {
      setError(err?.message ?? "Something went wrong");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid h-full grid-cols-1 lg:grid-cols-[1.15fr_0.85fr]">
      {/* Brand panel */}
      <div className="relative hidden flex-col justify-between overflow-hidden p-12 lg:flex">
        <div className="stagger">
          <div className="chip" style={{ borderColor: "rgba(255,107,53,0.4)", color: "var(--color-ember-400)" }}>
            LOCAL-FIRST · AGENT-NATIVE
          </div>
          <h1 className="font-display mt-8 text-6xl leading-[0.95]" style={{ fontWeight: 500 }}>
            Your business,
            <br />
            <em style={{ color: "var(--color-ember-500)" }}>on one desktop.</em>
          </h1>
          <p className="mt-6 max-w-md text-lg opacity-70">
            Businex is the operating layer where people and AI agents run the work —
            CRM, projects, documents, invoices, channels and terminals in fast, movable windows.
          </p>
        </div>
        <div className="stagger grid max-w-lg grid-cols-2 gap-4">
          {[
            ["Zero infrastructure", "One Node process and a SQLite file."],
            ["Agents as teammates", "OpenClaw + open-tag channels built in."],
            ["Real terminal", "PTY-backed xterm.js, tabs, streaming."],
            ["Everything searchable", "FTS5 across every record you own."],
          ].map(([t, d]) => (
            <div key={t} className="card">
              <div className="text-sm font-semibold">{t}</div>
              <div className="mt-1 text-xs opacity-60">{d}</div>
            </div>
          ))}
        </div>
      </div>

      {/* Auth card */}
      <div className="flex items-center justify-center p-8">
        <div className="panel w-full max-w-md rounded-3xl p-8">
          <div className="font-display text-3xl">Businex</div>
          <p className="mt-1 text-sm opacity-60">
            {mode === "login" ? "Sign in to your workspace." : "Create your workspace in seconds."}
          </p>

          <div className="mt-6 flex gap-2">
            {(["login", "register"] as const).map((m) => (
              <button
                key={m}
                className={"btn " + (mode === m ? "btn-primary" : "")}
                onClick={() => { setMode(m); setError(null); }}
                type="button"
              >
                {m === "login" ? "Sign in" : "Create account"}
              </button>
            ))}
          </div>

          <form onSubmit={submit} className="mt-6 flex flex-col gap-4">
            {mode === "register" && (
              <>
                <label className="block">
                  <span className="label">Your name</span>
                  <input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Ada Lovelace" required />
                </label>
                <label className="block">
                  <span className="label">Workspace name</span>
                  <input className="input" value={workspaceName} onChange={(e) => setWorkspaceName(e.target.value)} placeholder="Acme Inc" />
                </label>
              </>
            )}
            <label className="block">
              <span className="label">Email</span>
              <input className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="you@company.com" required />
            </label>
            <label className="block">
              <span className="label">Password</span>
              <input className="input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="••••••••" required minLength={8} />
            </label>

            {error && (
              <div className="text-sm" style={{ color: "var(--color-rose-500)" }}>{error}</div>
            )}

            <button className="btn btn-primary justify-center" disabled={busy} type="submit">
              {busy ? "Working…" : mode === "login" ? "Sign in" : "Create workspace"}
            </button>
          </form>

          <p className="mt-6 text-xs opacity-45">
            Runs entirely on your machine. Your data never leaves your network.
          </p>
        </div>
      </div>
    </div>
  );
}
