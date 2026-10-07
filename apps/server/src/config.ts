import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { randomBytes } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const dataDir = process.env.BUSINEX_DATA_DIR ?? path.resolve(here, "..", "data");

mkdirSync(dataDir, { recursive: true });

/** Secrets persist in the data dir so sessions survive restarts. Never logged, never committed. */
function loadSecret(): string {
  const secretPath = path.join(dataDir, "secret.key");
  if (existsSync(secretPath)) return readFileSync(secretPath, "utf8").trim();
  const secret = randomBytes(32).toString("base64url");
  writeFileSync(secretPath, secret, { mode: 0o600 });
  return secret;
}

export const config = {
  env: process.env.NODE_ENV ?? "development",
  host: process.env.BUSINEX_HOST ?? "127.0.0.1",
  port: Number(process.env.BUSINEX_PORT ?? 8788),
  webOrigin: process.env.BUSINEX_WEB_ORIGIN ?? "http://localhost:5199",
  dataDir,
  dbPath: process.env.BUSINEX_DB_PATH ?? path.join(dataDir, "businex.db"),
  uploadsDir: process.env.BUSINEX_UPLOADS_DIR ?? path.join(dataDir, "uploads"),
  secret: loadSecret(),
  sessionTtlMs: 1000 * 60 * 60 * 24 * 30,
  tokenPrefix: "bnx_",
};
