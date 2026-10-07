import React, { createContext, useContext, useEffect, useMemo, useState, useCallback } from "react";
import { api, setWorkspaceId } from "./api";
import { realtime } from "./ws";

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

export interface SessionUser {
  id: string;
  email: string;
  name: string;
  avatarColor: string;
}

export interface SessionWorkspace {
  id: string;
  name: string;
  slug: string;
  role: string;
}

interface SessionState {
  user: SessionUser | null;
  workspaces: SessionWorkspace[];
  workspace: SessionWorkspace | null;
  theme: "dark" | "light";
  loading: boolean;
  login: (email: string, password: string) => Promise<void>;
  register: (email: string, name: string, password: string, workspaceName?: string) => Promise<void>;
  logout: () => Promise<void>;
  switchWorkspace: (id: string) => void;
  toggleTheme: () => void;
  refresh: () => Promise<void>;
}

const SessionContext = createContext<SessionState | null>(null);

export function SessionProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<SessionUser | null>(null);
  const [workspaces, setWorkspaces] = useState<SessionWorkspace[]>([]);
  const [workspace, setWorkspace] = useState<SessionWorkspace | null>(null);
  const [loading, setLoading] = useState(true);
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    return (localStorage.getItem("businex.theme") as "dark" | "light") ?? "dark";
  });

  useEffect(() => {
    document.documentElement.classList.toggle("light", theme === "light");
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("businex.theme", theme);
  }, [theme]);

  const refresh = useCallback(async () => {
    try {
      const me = await api.get<{ user: SessionUser; workspaces: SessionWorkspace[] }>("/auth/me");
      setUser(me.user);
      setWorkspaces(me.workspaces);
      const saved = localStorage.getItem("businex.workspace");
      const next = me.workspaces.find((w) => w.id === saved) ?? me.workspaces[0] ?? null;
      setWorkspace(next);
      setWorkspaceId(next?.id ?? null);
      if (next) {
        realtime.connect(next.id);
      }
    } catch {
      setUser(null);
      setWorkspaces([]);
      setWorkspace(null);
      setWorkspaceId(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    return () => realtime.disconnect();
  }, [refresh]);

  const login = useCallback(async (email: string, password: string) => {
    await api.post("/auth/login", { email, password });
    await refresh();
  }, [refresh]);

  const register = useCallback(async (email: string, name: string, password: string, workspaceName?: string) => {
    await api.post("/auth/register", { email, name, password, workspaceName });
    await refresh();
  }, [refresh]);

  const logout = useCallback(async () => {
    await api.post("/auth/logout").catch(() => undefined);
    realtime.disconnect();
    setUser(null);
    setWorkspaces([]);
    setWorkspace(null);
    localStorage.removeItem("businex.windows");
  }, []);

  const switchWorkspace = useCallback((id: string) => {
    const next = workspaces.find((w) => w.id === id) ?? null;
    setWorkspace(next);
    setWorkspaceId(next?.id ?? null);
    localStorage.setItem("businex.workspace", id);
    realtime.disconnect();
    if (next) realtime.connect(next.id);
  }, [workspaces]);

  const value = useMemo<SessionState>(() => ({
    user, workspaces, workspace, theme, loading,
    login, register, logout, switchWorkspace,
    toggleTheme: () => setTheme((t) => (t === "dark" ? "light" : "dark")),
    refresh,
  }), [user, workspaces, workspace, theme, loading, login, register, logout, switchWorkspace, refresh]);

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionState {
  const ctx = useContext(SessionContext);
  if (!ctx) throw new Error("useSession outside provider");
  return ctx;
}

// ---------------------------------------------------------------------------
// Windows (desktop state, persisted per browser)
// ---------------------------------------------------------------------------

export interface WindowState {
  id: string;
  module: string;
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
  z: number;
  minimized: boolean;
  maximized: boolean;
}

interface WindowsState {
  windows: WindowState[];
  open: (module: string, title: string) => void;
  close: (id: string) => void;
  focus: (id: string) => void;
  setBounds: (id: string, bounds: Partial<Pick<WindowState, "x" | "y" | "w" | "h">>) => void;
  minimize: (id: string) => void;
  toggleMaximize: (id: string) => void;
}

const WindowsContext = createContext<WindowsState | null>(null);

const WINDOWS_KEY = "businex.windows";

export function WindowsProvider({ children }: { children: React.ReactNode }) {
  const [windows, setWindows] = useState<WindowState[]>(() => {
    try {
      return JSON.parse(localStorage.getItem(WINDOWS_KEY) ?? "[]");
    } catch {
      return [];
    }
  });
  const [topZ, setTopZ] = useState(() => Math.max(10, ...windows.map((w) => w.z)));

  useEffect(() => {
    const persist = () => localStorage.setItem(WINDOWS_KEY, JSON.stringify(windows));
    const timer = setTimeout(persist, 200);
    window.addEventListener("pagehide", persist);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("pagehide", persist);
    };
  }, [windows]);

  const open = useCallback((module: string, title: string) => {
    setWindows((current) => {
      const existing = current.find((w) => w.module === module);
      if (existing) {
        return current.map((w) => (w.id === existing.id ? { ...w, minimized: false, z: topZ + 1 } : w));
      }
      const offset = (current.length % 6) * 28;
      const next: WindowState = {
        id: module + "-" + Date.now().toString(36),
        module,
        title,
        x: 84 + offset,
        y: 74 + offset,
        w: Math.min(1080, Math.max(720, window.innerWidth - 220)),
        h: Math.min(720, Math.max(480, window.innerHeight - 190)),
        z: topZ + 1,
        minimized: false,
        maximized: false,
      };
      setTopZ(topZ + 1);
      return [...current, next];
    });
  }, [topZ]);

  const close = useCallback((id: string) => setWindows((c) => c.filter((w) => w.id !== id)), []);
  const focus = useCallback((id: string) => {
    setWindows((c) => c.map((w) => (w.id === id ? { ...w, z: topZ + 1, minimized: false } : w)));
    setTopZ((z) => z + 1);
  }, [topZ]);
  const setBounds = useCallback((id: string, bounds: Partial<Pick<WindowState, "x" | "y" | "w" | "h">>) => {
    setWindows((c) => c.map((w) => (w.id === id ? { ...w, ...bounds } : w)));
  }, []);
  const minimize = useCallback((id: string) => {
    setWindows((c) => c.map((w) => (w.id === id ? { ...w, minimized: true } : w)));
  }, []);
  const toggleMaximize = useCallback((id: string) => {
    setWindows((c) => c.map((w) => (w.id === id ? { ...w, maximized: !w.maximized } : w)));
  }, []);

  const value = useMemo<WindowsState>(() => ({
    windows, open, close, focus, setBounds, minimize, toggleMaximize,
  }), [windows, open, close, focus, setBounds, minimize, toggleMaximize]);

  return <WindowsContext.Provider value={value}>{children}</WindowsContext.Provider>;
}

export function useWindows(): WindowsState {
  const ctx = useContext(WindowsContext);
  if (!ctx) throw new Error("useWindows outside provider");
  return ctx;
}

// ---------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------

interface Toast { id: number; text: string; kind: "info" | "error" | "success" }
const ToastContext = createContext<{ toasts: Toast[]; push: (text: string, kind?: Toast["kind"]) => void } | null>(null);

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const push = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4200);
  }, []);
  const value = useMemo(() => ({ toasts, push }), [toasts, push]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div className="fixed bottom-5 right-5 z-[10000] flex flex-col gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            className="panel fade-in rounded-xl px-4 py-3 text-sm shadow-lg"
            style={{ borderLeft: "3px solid " + (t.kind === "error" ? "var(--color-rose-500)" : t.kind === "success" ? "var(--color-sage-500)" : "var(--color-ember-500)") }}
          >
            {t.text}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast outside provider");
  return ctx;
}
