import { EventEmitter } from "node:events";

export interface BusinexEvent {
  type: string;
  workspaceId: string;
  payload: unknown;
  at: string;
}

class EventBus extends EventEmitter {
  publish(type: string, workspaceId: string, payload: unknown): void {
    const event: BusinexEvent = { type, workspaceId, payload, at: new Date().toISOString() };
    this.emit("event", event);
  }
}

export const bus = new EventBus();
bus.setMaxListeners(200);
