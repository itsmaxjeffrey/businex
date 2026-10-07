import { getDb } from "../db";

/**
 * Upsert a record into the cross-entity FTS index. Called from mutation
 * handlers so search results stay current without table triggers.
 */
export function indexEntity(
  workspaceId: string,
  entityType: string,
  entityId: string,
  title: string,
  body: string,
): void {
  const db = getDb();
  db.prepare("DELETE FROM search_index WHERE entity_type = ? AND entity_id = ?").run(entityType, entityId);
  db.prepare(`
    INSERT INTO search_index (entity_type, entity_id, workspace_id, title, body)
    VALUES (?, ?, ?, ?, ?)
  `).run(entityType, entityId, workspaceId, title, body);
}

export function removeFromIndex(entityType: string, entityId: string): void {
  getDb().prepare("DELETE FROM search_index WHERE entity_type = ? AND entity_id = ?").run(entityType, entityId);
}

export interface SearchHit {
  entityType: string;
  entityId: string;
  title: string;
  snippet: string;
  score: number;
}

export function search(workspaceId: string, query: string, limit = 20): SearchHit[] {
  const q = query.replace(/"/g, '""').trim();
  if (!q) return [];
  const rows = getDb().prepare(`
    SELECT entity_type, entity_id, title,
           snippet(search_index, 4, '[', ']', '…', 12) AS snippet,
           bm25(search_index) AS score
    FROM search_index
    WHERE search_index MATCH ? AND workspace_id = ?
    ORDER BY score
    LIMIT ?
  `).all('"' + q + '"*', workspaceId, limit) as any[];
  return rows.map((r) => ({
    entityType: r.entity_type,
    entityId: r.entity_id,
    title: r.title,
    snippet: r.snippet,
    score: r.score,
  }));
}
