import Database from "better-sqlite3";
import { config } from "../config";
import { migrate } from "./migrate";

let db: Database.Database | null = null;

export function getDb(): Database.Database {
  if (db) return db;
  db = new Database(config.dbPath);
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");
  db.pragma("busy_timeout = 5000");
  db.pragma("synchronous = NORMAL");
  migrate(db);
  return db;
}

export function now(): string {
  return new Date().toISOString();
}

/** SQLite stores booleans as 0/1. */
export function bool(v: unknown): boolean {
  return v === 1 || v === true;
}

export function parseJson<T>(v: unknown, fallback: T): T {
  if (typeof v !== "string") return fallback;
  try { return JSON.parse(v) as T; } catch { return fallback; }
}

export function closeDb(): void {
  if (db) { db.close(); db = null; }
}
