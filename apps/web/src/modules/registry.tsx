import type React from "react";
import {
  IconDashboard, IconCrm, IconProjects, IconDocs, IconCalendar,
  IconInvoice, IconChannels, IconAgent, IconTerminal, IconSettings,
} from "../components/icons";
import { DashboardModule } from "./Dashboard";
import { CrmModule } from "./Crm";
import { ProjectsModule } from "./Projects";
import { DocumentsModule } from "./Documents";
import { CalendarModule } from "./Calendar";
import { InvoicesModule } from "./Invoices";
import { ChannelsModule } from "./Channels";
import { AgentsModule } from "./Agents";
import { TerminalModule } from "./Terminal";
import { SettingsModule } from "./Settings";

export interface ModuleDef {
  id: string;
  title: string;
  description: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  component: React.ComponentType;
}

export const modules: ModuleDef[] = [
  { id: "dashboard", title: "Mission Control", description: "Business pulse: pipeline, work, activity", icon: IconDashboard, component: DashboardModule },
  { id: "crm", title: "CRM", description: "Contacts, companies, deals pipeline", icon: IconCrm, component: CrmModule },
  { id: "projects", title: "Projects", description: "Projects and kanban tasks", icon: IconProjects, component: ProjectsModule },
  { id: "documents", title: "Documents", description: "Knowledge base with full-text search", icon: IconDocs, component: DocumentsModule },
  { id: "calendar", title: "Calendar", description: "Events and scheduling", icon: IconCalendar, component: CalendarModule },
  { id: "invoices", title: "Invoices", description: "Invoices, line items, status", icon: IconInvoice, component: InvoicesModule },
  { id: "channels", title: "Channels", description: "open-tag style human + agent chat", icon: IconChannels, component: ChannelsModule },
  { id: "agents", title: "Agents", description: "OpenClaw console and agent teammates", icon: IconAgent, component: AgentsModule },
  { id: "terminal", title: "Terminal", description: "Real PTY terminals", icon: IconTerminal, component: TerminalModule },
  { id: "settings", title: "Settings", description: "Workspace, members, API keys, audit", icon: IconSettings, component: SettingsModule },
];
