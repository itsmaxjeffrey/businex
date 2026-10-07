// Realtime client: reconnecting WebSocket with event subscription.

import { isDemoMode } from "./demo";

type Handler = (event: any) => void;

class Realtime {
  private ws: WebSocket | null = null;
  private handlers = new Set<Handler>();
  private terminalHandlers = new Set<Handler>();
  private workspaceId: string | null = null;
  private shouldRun = false;

  connect(workspaceId: string): void {
    if (isDemoMode()) return; // no backend behind the public demo
    this.workspaceId = workspaceId;
    this.shouldRun = true;
    this.open();
  }

  disconnect(): void {
    this.shouldRun = false;
    this.ws?.close();
    this.ws = null;
  }

  private open(): void {
    if (!this.shouldRun || !this.workspaceId) return;
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const url = proto + "://" + location.host + "/ws?workspaceId=" + encodeURIComponent(this.workspaceId);
    const ws = new WebSocket(url);
    this.ws = ws;

    ws.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data);
        if (data.type === "event") {
          for (const h of this.handlers) h(data.event);
          if (String(data.event?.type ?? "").startsWith("terminal.")) {
            for (const h of this.terminalHandlers) h(data.event);
          }
        }
      } catch { /* ignore malformed frames */ }
    };
    ws.onclose = () => {
      if (this.shouldRun) setTimeout(() => this.open(), 1200);
    };
  }

  send(payload: Record<string, unknown>): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(payload));
    }
  }

  onEvent(handler: Handler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  onTerminal(handler: Handler): () => void {
    this.terminalHandlers.add(handler);
    return () => this.terminalHandlers.delete(handler);
  }
}

export const realtime = new Realtime();
