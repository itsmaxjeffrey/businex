import React, { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { api } from "../lib/api";
import { realtime } from "../lib/ws";
import { useToast } from "../lib/store";
import { IconPlus } from "../components/icons";

interface Tab { id: string; label: string; term: Terminal; fit: FitAddon; dispose: () => void }

export function TerminalModule() {
  const { push } = useToast();
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const tabsRef = useRef<Tab[]>([]);

  useEffect(() => { tabsRef.current = tabs; }, [tabs]);

  const spawn = async () => {
    try {
      const session = await api.post<any>("/terminals", { cols: 120, rows: 32 });
      const term = new Terminal({
        fontSize: 13,
        fontFamily: "var(--font-mono), monospace",
        theme: {
          background: "#0b0c0e",
          foreground: "#f2ede4",
          cursor: "#ff6b35",
          selectionBackground: "rgba(255,107,53,0.35)",
        },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);

      const tab: Tab = {
        id: session.id,
        label: "term " + (tabsRef.current.length + 1),
        term,
        fit,
        dispose: () => {
          term.dispose();
          realtime.send({ type: "terminal.kill", terminalId: session.id });
        },
      };

      term.onData((data) => realtime.send({ type: "terminal.input", terminalId: session.id, data }));

      setTabs((t) => [...t, tab]);
      setActiveId(session.id);
    } catch (e: any) {
      push("Could not start terminal: " + e.message, "error");
    }
  };

  // Attach the active terminal to the DOM and watch its stream.
  useEffect(() => {
    const tab = tabs.find((t) => t.id === activeId);
    const host = hostRef.current;
    if (!tab || !host) return;
    host.innerHTML = "";
    tab.term.open(host);
    try { tab.fit.fit(); } catch { /* host may be hidden */ }

    realtime.send({ type: "terminal.watch", terminalId: tab.id });

    const off = realtime.onTerminal((event) => {
      if (event.payload?.terminalId !== tab.id) return;
      if (event.type === "terminal.data") tab.term.write(event.payload.data);
      if (event.type === "terminal.exit") tab.term.write("\r\n[process exited]\r\n");
    });

    const onResize = () => {
      try {
        tab.fit.fit();
        realtime.send({ type: "terminal.resize", terminalId: tab.id, cols: tab.term.cols, rows: tab.term.rows });
      } catch { /* ignore */ }
    };
    window.addEventListener("resize", onResize);
    onResize();

    return () => {
      off();
      window.removeEventListener("resize", onResize);
      realtime.send({ type: "terminal.unwatch", terminalId: tab.id });
    };
  }, [activeId, tabs]);

  const closeTab = (tab: Tab) => {
    tab.dispose();
    setTabs((t) => t.filter((x) => x.id !== tab.id));
    if (activeId === tab.id) setActiveId(tabsRef.current.find((x) => x.id !== tab.id)?.id ?? null);
  };

  return (
    <div className="flex h-full flex-col" style={{ background: "#0b0c0e" }}>
      <div className="flex items-center gap-2 border-b px-3 py-2" style={{ borderColor: "var(--panel-border)" }}>
        {tabs.map((t) => (
          <div
            key={t.id}
            className="flex cursor-pointer items-center gap-2 rounded-lg px-3 py-1 text-xs"
            style={{ background: activeId === t.id ? "rgba(255,107,53,0.16)" : "transparent" }}
            onClick={() => setActiveId(t.id)}
          >
            <span className="font-mono">{t.label}</span>
            <button className="opacity-50 hover:opacity-100" onClick={(e) => { e.stopPropagation(); closeTab(t); }}>✕</button>
          </div>
        ))}
        <button className="btn btn-ghost text-xs" onClick={spawn}><IconPlus size={13} /> New terminal</button>
      </div>
      <div ref={hostRef} className="flex-1 overflow-hidden p-2" />
      {tabs.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="text-center opacity-60">
            <div className="font-display text-2xl">Terminal</div>
            <p className="mt-1 text-sm">Real PTY sessions on your machine.</p>
            <button className="btn btn-primary mt-4" onClick={spawn}>Start a terminal</button>
          </div>
        </div>
      )}
    </div>
  );
}
