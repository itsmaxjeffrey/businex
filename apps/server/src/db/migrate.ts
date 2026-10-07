import type { Database } from "better-sqlite3";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

/**
 * Ordered migrations. Each entry is applied exactly once and recorded in the
 * schema_migrations ledger. Keep migrations additive and idempotent.
 */
const migrations: Array<{ id: string; file: string }> = [
  { id: "0001_initial_schema", file: "schema.sql" },
  { id: "0002_auth_extras", file: "0002_auth_extras.sql" },
];

export function migrate(db: Database): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_migrations (
      id         TEXT PRIMARY KEY,
      applied_at TEXT NOT NULL
    );
  `);
  const applied = new Set(
    db.prepare("SELECT id FROM schema_migrations").all().map((r: any) => r.id),
  );
  for (const m of migrations) {
    if (applied.has(m.id)) continue;
    const sql = readFileSync(path.join(here, m.file), "utf8");
    const run = db.transaction(() => {
      db.exec(sql);
      db.prepare("INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)").run(m.id, new Date().toISOString());
    });
    run();
  }
}
