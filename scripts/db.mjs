#!/usr/bin/env node
// Database utilities: node scripts/db.mjs status | path | reset
import { existsSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dataDir = process.env.BUSINEX_DATA_DIR ?? path.join(root, "apps/server/data");
const dbPath = process.env.BUSINEX_DB_PATH ?? path.join(dataDir, "businex.db");

const command = process.argv[2] ?? "status";

switch (command) {
  case "path":
    console.log(dbPath);
    break;
  case "status":
    console.log("data dir : " + dataDir);
    console.log("database : " + dbPath + (existsSync(dbPath) ? "" : "  (not created yet)"));
    break;
  case "reset":
    if (process.argv[3] !== "--yes") {
      console.log("This deletes the local database. Re-run with: node scripts/db.mjs reset --yes");
      process.exit(1);
    }
    for (const suffix of ["", "-wal", "-shm"]) {
      const file = dbPath + suffix;
      if (existsSync(file)) rmSync(file);
    }
    console.log("database removed");
    break;
  default:
    console.log("usage: node scripts/db.mjs [status|path|reset --yes]");
}
