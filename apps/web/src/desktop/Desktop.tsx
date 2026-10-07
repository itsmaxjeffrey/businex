import React, { Suspense, useEffect, useState } from "react";
import { useSession, useWindows } from "../lib/store";
import { modules } from "../modules/registry";
import { WindowFrame } from "./Window";
import { CommandPalette } from "./CommandPalette";
import { Avatar, Spinner } from "../components/ui";
import { IconSearch, IconSun, IconMoon } from "../components/icons";

class ModuleBoundary extends React.Component<{ label: string; children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    if (this.state.failed) return <div className="p-6">
      <p>Couldn't open {this.props.label}. Reload the page to try again.</p>
      <button className="btn btn-primary mt-3" onClick={() => window.location.reload()}>Reload page</button>
    </div>;
    return this.props.children;
  }
}

function Clock() {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 30000);
    return () => clearInterval(t);
  }, []);
  return (
    <div className="text-right text-xs leading-tight">
      <div className="font-mono">{now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</div>
      <div className="opacity-45">{now.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })}</div>
    </div>
  );
}

export function Desktop() {
  const { user, workspace, workspaces, switchWorkspace, logout, theme, toggleTheme } = useSession();
  const { windows, open, focus, close } = useWindows();
  const [paletteOpen, setPaletteOpen] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // First visit: open the dashboard window.
  useEffect(() => {
    if (windows.length === 0) open("dashboard", "Mission Control");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="relative h-full w-full overflow-hidden">
      {/* Top bar */}
      <header
        className="absolute inset-x-0 top-0 z-[400] flex h-[52px] items-center gap-4 px-4"
        style={{ background: "linear-gradient(180deg, rgba(11,12,14,0.85), rgba(11,12,14,0.35))", borderBottom: "1px solid var(--panel-border)", backdropFilter: "blur(14px)" }}
      >
        <div className="flex items-center gap-2">
          <div
            className="flex h-8 w-8 items-center justify-center rounded-xl font-display text-sm"
            style={{ background: "linear-gradient(135deg, var(--color-ember-500), var(--color-ember-600))", color: "#1a0d06" }}
          >
            Bx
          </div>
          <select
            className="select"
            style={{ width: "auto", padding: "0.3rem 0.6rem", fontSize: "0.8rem" }}
            value={workspace?.id ?? ""}
            onChange={(e) => switchWorkspace(e.target.value)}
          >
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>{w.name}</option>
            ))}
          </select>
        </div>

        <button
          className="btn btn-ghost ml-2 gap-2 opacity-70"
          onClick={() => setPaletteOpen(true)}
        >
          <IconSearch size={15} />
          <span className="text-xs opacity-70">Search everything</span>
          <span className="kbd">⌘K</span>
        </button>

        <div className="ml-auto flex items-center gap-3">
          <Clock />
          <button className="btn btn-ghost" onClick={toggleTheme} aria-label="Toggle theme">
            {theme === "dark" ? <IconSun size={17} /> : <IconMoon size={17} />}
          </button>
          <div className="flex items-center gap-2">
            <Avatar name={user?.name ?? "?"} color={user?.avatarColor} size={30} />
            <button className="btn btn-ghost text-xs opacity-70" onClick={() => void logout()}>Sign out</button>
          </div>
        </div>
      </header>

      {/* Left rail */}
      <nav
        className="absolute bottom-0 left-0 top-[52px] z-[350] flex w-[64px] flex-col items-center gap-2 py-4"
        style={{ borderRight: "1px solid var(--panel-border)", background: "rgba(11,12,14,0.35)", backdropFilter: "blur(10px)" }}
      >
        {modules.map((m) => {
          const isOpen = windows.some((w) => w.module === m.id);
          return (
            <button
              key={m.id}
              className={"rail-item " + (isOpen ? "active" : "")}
              title={m.title}
              onPointerEnter={() => { void m.preload?.().catch(() => {}); }}
              onFocus={() => { void m.preload?.().catch(() => {}); }}
              onClick={() => open(m.id, m.title)}
            >
              <m.icon size={19} />
            </button>
          );
        })}
      </nav>

      {/* Window layer */}
      <main className="absolute bottom-[70px] left-[64px] right-0 top-[52px]">
        {windows.map((win) => {
          const mod = modules.find((m) => m.id === win.module);
          if (!mod) return null;
          return (
            <WindowFrame key={win.id} win={win}>
              <ModuleBoundary label={mod.title}>
              <Suspense fallback={<Spinner label={"Opening " + mod.title + "…"} />}>
                <mod.component />
              </Suspense>
              </ModuleBoundary>
            </WindowFrame>
          );
        })}
      </main>

      {/* Dock */}
      <footer
        className="absolute bottom-3 left-1/2 z-[420] flex -translate-x-1/2 items-center gap-2 rounded-2xl px-3 py-2"
        style={{ background: "rgba(20,23,28,0.75)", border: "1px solid var(--panel-border)", backdropFilter: "blur(18px)" }}
      >
        {modules.map((m) => {
          const win = windows.find((w) => w.module === m.id);
          return (
            <button
              key={m.id}
              className="rail-item"
              style={{ width: 38, height: 38, opacity: win ? 1 : 0.55 }}
              title={m.title}
              onPointerEnter={() => { void m.preload?.().catch(() => {}); }}
              onFocus={() => { void m.preload?.().catch(() => {}); }}
              onClick={() => (win ? (win.minimized ? open(m.id, m.title) : focus(win.id)) : open(m.id, m.title))}
            >
              <m.icon size={17} />
              {win && (
                <span className="absolute bottom-[3px] h-[3px] w-[3px] rounded-full" style={{ background: "var(--color-ember-500)" }} />
              )}
            </button>
          );
        })}
        <div className="mx-1 h-6 w-px" style={{ background: "var(--panel-border)" }} />
        {windows.filter((w) => w.minimized).map((w) => (
          <button key={w.id} className="chip" onClick={() => open(w.module, w.title)}>
            {w.title}
          </button>
        ))}
      </footer>

      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} />}
    </div>
  );
}
