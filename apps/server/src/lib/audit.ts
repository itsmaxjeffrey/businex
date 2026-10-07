import { getDb, now } from "../db";
import { newId } from "./id";

export function audit(
  workspaceId: string,
  actor: { type: "user" | "agent" | "system"; id: string | null },
  action: string,
  entityType: string,
  entityId: string | null,
  meta?: Record<string, unknown>,
): void {
  getDb().prepare(`
    INSERT INTO audit_log (id, workspace_id, actor_type, actor_id, action, entity_type, entity_id, meta, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(newId("aud"), workspaceId, actor.type, actor.id, action, entityType, entityId, meta ? JSON.stringify(meta) : null, now());
}
