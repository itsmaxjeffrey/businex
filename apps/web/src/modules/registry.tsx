import React, { lazy, memo } from "react";
import {
  IconDashboard, IconCrm, IconProjects, IconDocs, IconCalendar,
  IconInvoice, IconChannels, IconAgent, IconTerminal, IconSettings,
} from "../components/icons";
import { DashboardModule } from "./Dashboard";

export interface ModuleDef {
  id: string;
  title: string;
  description: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  component: React.ComponentType;
  preload?: () => Promise<{ default: React.ComponentType }>;
}

// Share the preload promise with React.lazy, including pointer/focus prefetches.
function deferred(load: () => Promise<{ default: React.ComponentType }>) {
  let pending: ReturnType<typeof load> | undefined;
  const preload = () => pending ??= load();
  return { component: memo(lazy(preload)), preload };
}

export const modules: ModuleDef[] = [
  { id: "dashboard", title: "Mission Control", description: "Business pulse: pipeline, work, activity", icon: IconDashboard, component: memo(DashboardModule) },
  { id: "crm", title: "CRM", description: "Contacts, companies, deals pipeline", icon: IconCrm, ...deferred(() => import("./Crm").then(m => ({ default: m.CrmModule }))) },
  { id: "projects", title: "Projects", description: "Projects and kanban tasks", icon: IconProjects, ...deferred(() => import("./Projects").then(m => ({ default: m.ProjectsModule }))) },
  { id: "documents", title: "Documents", description: "Knowledge base with full-text search", icon: IconDocs, ...deferred(() => import("./Documents").then(m => ({ default: m.DocumentsModule }))) },
  { id: "calendar", title: "Calendar", description: "Events and scheduling", icon: IconCalendar, ...deferred(() => import("./Calendar").then(m => ({ default: m.CalendarModule }))) },
  { id: "invoices", title: "Invoices", description: "Invoices, line items, status", icon: IconInvoice, ...deferred(() => import("./Invoices").then(m => ({ default: m.InvoicesModule }))) },
  { id: "channels", title: "Channels", description: "open-tag style human + agent chat", icon: IconChannels, ...deferred(() => import("./Channels").then(m => ({ default: m.ChannelsModule }))) },
  { id: "agents", title: "Agents", description: "OpenClaw console and agent teammates", icon: IconAgent, ...deferred(() => import("./Agents").then(m => ({ default: m.AgentsModule }))) },
  { id: "terminal", title: "Terminal", description: "Real PTY terminals", icon: IconTerminal, ...deferred(() => import("./Terminal").then(m => ({ default: m.TerminalModule }))) },
  { id: "settings", title: "Settings", description: "Workspace, members, API keys, audit", icon: IconSettings, ...deferred(() => import("./Settings").then(m => ({ default: m.SettingsModule }))) },
];
