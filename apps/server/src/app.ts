import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";
import type { Env } from "./context";
import { jsonError } from "./lib/errors";
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

  app.use("*", cors({
    origin: (origin) => origin || "*",
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

  app.notFound((c) => c.json({ error: { code: "not_found", message: "Route not found" } }, 404));
  return app;
}
