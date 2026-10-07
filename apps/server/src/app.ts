import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";
import { serveStatic } from "@hono/node-server/serve-static";
import { secureHeaders } from "hono/secure-headers";
import { config } from "./config";
import type { Env } from "./context";
import { jsonError, forbidden } from "./lib/errors";
import { authRoutes } from "./routes/auth";
import { workspaceRoutes } from "./routes/workspaces";
import { crmRoutes } from "./routes/crm";
import { projectRoutes } from "./routes/projects";
import { documentRoutes } from "./routes/documents";
import { calendarRoutes } from "./routes/calendar";
import { invoiceRoutes } from "./routes/invoices";
import { channelRoutes, messageRoutes } from "./routes/channels";
import { agentRoutes } from "./routes/agents";
import { searchRoutes } from "./routes/search";
import { analyticsRoutes } from "./routes/analytics";
import { terminalRoutes } from "./routes/terminal";
import { mcpRoutes } from "./routes/mcp";
import { openTagRoutes } from "./routes/opentag";

export function createApp(): Hono<Env> {
  const app = new Hono<Env>();

  app.use("*", secureHeaders());
  app.use("*", async (c, next) => {
    const origin = c.req.header("origin");
    if (origin && origin !== config.webOrigin && !["GET", "HEAD", "OPTIONS"].includes(c.req.method)) {
      throw forbidden("Request origin is not allowed");
    }
    await next();
  });
  app.use("*", cors({
    origin: (origin) => origin === config.webOrigin ? origin : undefined,
    credentials: true,
  }));
  if (process.env.NODE_ENV !== "test") app.use("*", logger());

  app.onError((err, c) => jsonError(c, err));

  app.get("/api/health", (c) => c.json({
    ok: true,
    name: "businex",
    version: "0.1.0",
    uptime: process.uptime(),
  }));

  app.route("/api/auth", authRoutes);
  app.route("/api/workspace", workspaceRoutes);
  app.route("/api/crm", crmRoutes);
  app.route("/api/projects", projectRoutes);
  app.route("/api/documents", documentRoutes);
  app.route("/api/calendar", calendarRoutes);
  app.route("/api/invoices", invoiceRoutes);
  app.route("/api/channels", channelRoutes);
  app.route("/api/messages", messageRoutes);
  app.route("/api", agentRoutes);
  app.route("/api", searchRoutes);
  app.route("/api", analyticsRoutes);
  app.route("/api/terminals", terminalRoutes);
  app.route("/api", mcpRoutes);
  app.route("/api", openTagRoutes);

  if (config.staticDir) {
    const root = config.staticDir;
    app.get("*", async (c, next) => {
      if (c.req.path === "/api" || c.req.path.startsWith("/api/") || c.req.path === "/ws") return next();
      return serveStatic({ root })(c, next);
    });
    app.get("*", async (c, next) => {
      if (c.req.path === "/api" || c.req.path.startsWith("/api/") || c.req.path === "/ws" || c.req.path.startsWith("/assets/")) return next();
      return serveStatic({ root, path: "index.html" })(c, next);
    });
  }

  app.notFound((c) => c.json({ error: { code: "not_found", message: "Route not found" } }, 404));
  return app;
}
