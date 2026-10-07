import { serve } from "@hono/node-server";
import { createApp } from "./app";
import { getDb } from "./db";
import { attachWebSocket } from "./ws";
import { killAllTerminals } from "./terminal";
import { config } from "./config";
import { redisBridge } from "./lib/redis";

// Touch the database early so schema migrations run before the first request.
getDb();

const app = createApp();
const server = serve({ fetch: app.fetch, hostname: config.host, port: config.port }, (info) => {
  console.log("[businex] server listening on http://" + info.address + ":" + info.port);
  console.log("[businex] data dir: " + config.dataDir);
});

attachWebSocket(server as any);
void redisBridge.start();

function shutdown() {
  console.log("[businex] shutting down");
  killAllTerminals();
  redisBridge.stop();
  process.exit(0);
}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
