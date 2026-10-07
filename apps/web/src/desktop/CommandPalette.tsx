import React, { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { useSession, useWindows, useToast } from "../lib/store";
import { modules } from "../modules/registry";

interface Command {
  id: string;
  title: string;
  hint?: string;
  run: () => void;
}

export function CommandPalette({ onClose }: { onClose: () => void }) {
  const { toggleTheme, logout } = useSession();
  const { open } = useWindows();
  const { push } = useToast();
  const [query, setQuery] = useState("");
  const [searchHits, setSearchHits] = useState<Array<{ entityType: string; entityId: string; title: string; snippet: string }>>([]);
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) { setSearchHits([]); return; }
    const t = setTimeout(() => {
      api.get<{ items: any[] }>("/search?q=" + encodeURIComponent(q) + "&limit=6")
        .then((r) => setSearchHits(r.items))
        .catch(() => setSearchHits([]));
    }, 160);
    return () => clearTimeout(t);
  }, [query]);

  const commands = useMemo<Command[]>(() => {
    const base: Command[] = [
      ...modules.map((m) => ({
        id: "open-" + m.id,
        title: "Open " + m.title,
        hint: m.description,
        run: () => open(m.id, m.title),
      })),
      { id: "theme", title: "Toggle light / dark theme", run: toggleTheme },
      { id: "logout", title: "Sign out", run: () => void logout() },
    ];
    const q = query.trim().toLowerCase();
    const filtered = q
      ? base.filter((c) => (c.title + " " + (c.hint ?? "")).toLowerCase().includes(q))
      : base;
    return filtered;
  }, [query, open, toggleTheme, logout]);

  const all = useMemo(() => ([
    ...commands,
    ...searchHits.map((h) => ({
      id: "hit-" + h.entityId,
      title: h.title,
      hint: h.entityType + " · " + h.snippet,
      run: () => push("Opening " + h.title + " (" + h.entityType + ")", "info"),
    })),
  ]), [commands, searchHits]);

  const runActive = () => {
    const cmd = all[active];
    if (cmd) { cmd.run(); onClose(); }
  };

  return (
    <div className="fixed inset-0 z-[9500] flex items-start justify-center pt-[12vh]" onMouseDown={onClose}>
      <div className="absolute inset-0" style={{ background: "rgba(6,7,9,0.5)", backdropFilter: "blur(6px)" }} />
      <div className="panel relative w-[620px] overflow-hidden rounded-2xl" onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-3 border-b px-5 py-4" style={{ borderColor: "var(--panel-border)" }}>
          <span className="opacity-50">⌘</span>
          <input
            ref={inputRef}
            className="w-full bg-transparent text-lg outline-none"
            placeholder="Search modules, records, actions…"
            value={query}
            onChange={(e) => { setQuery(e.target.value); setActive(0); }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setActive((a) => Math.min(a + 1, all.length - 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setActive((a) => Math.max(a - 1, 0)); }
              if (e.key === "Enter") { e.preventDefault(); runActive(); }
              if (e.key === "Escape") onClose();
            }}
          />
          <span className="kbd">esc</span>
        </div>
        <div className="max-h-[46vh] overflow-y-auto p-2">
          {all.length === 0 && (
            <div className="px-4 py-8 text-center text-sm opacity-50">No matches. Try another query.</div>
          )}
          {all.map((cmd, i) => (
            <button
              key={cmd.id}
              className="flex w-full items-center justify-between rounded-xl px-4 py-3 text-left text-sm"
              style={{ background: i === active ? "rgba(255,107,53,0.14)" : "transparent" }}
              onMouseEnter={() => setActive(i)}
              onClick={() => { cmd.run(); onClose(); }}
            >
              <span className="font-medium">{cmd.title}</span>
              {cmd.hint && <span className="ml-4 max-w-[45%] truncate text-xs opacity-50">{cmd.hint}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
