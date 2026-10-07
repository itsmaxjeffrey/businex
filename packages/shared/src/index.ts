// Businex shared domain types and validation helpers.
// Imported by both the server and the web client; keep it dependency-light (zod only).

import { z } from "zod";

// ---------------------------------------------------------------------------
// Identifiers and common shapes
// ---------------------------------------------------------------------------

export type ID = string;
export type ISODate = string;

export const ROLES = ["owner", "admin", "member", "agent", "viewer"] as const;
export type Role = (typeof ROLES)[number];

export const ROLE_RANK: Record<Role, number> = {
  owner: 100,
  admin: 80,
  member: 40,
  agent: 30,
  viewer: 10,
};

export function roleAtLeast(role: Role, required: Role): boolean {
  return ROLE_RANK[role] >= ROLE_RANK[required];
}

// ---------------------------------------------------------------------------
// API envelopes
// ---------------------------------------------------------------------------

export interface ApiError {
  error: { code: string; message: string; details?: unknown };
}

export interface Paginated<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

export interface User {
  id: ID;
  email: string;
  name: string;
  avatarColor: string;
  createdAt: ISODate;
  lastSeenAt: ISODate | null;
}

export interface Workspace {
  id: ID;
  orgId: ID;
  name: string;
  slug: string;
  createdAt: ISODate;
}

export interface Membership {
  id: ID;
  userId: ID;
  workspaceId: ID;
  role: Role;
  createdAt: ISODate;
}

export interface Company {
  id: ID;
  name: string;
  domain: string | null;
  industry: string | null;
  size: string | null;
  website: string | null;
  notes: string | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export interface Contact {
  id: ID;
  firstName: string;
  lastName: string;
  email: string | null;
  phone: string | null;
  title: string | null;
  companyId: ID | null;
  notes: string | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export const DEAL_STAGES = ["lead", "qualified", "proposal", "negotiation", "won", "lost"] as const;
export type DealStage = (typeof DEAL_STAGES)[number];

export interface Deal {
  id: ID;
  name: string;
  companyId: ID | null;
  contactId: ID | null;
  stage: DealStage;
  value: number;
  currency: string;
  closeDate: ISODate | null;
  ownerId: ID | null;
  notes: string | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export interface Activity {
  id: ID;
  type: string;
  subject: string;
  body: string | null;
  entityType: string;
  entityId: ID;
  dueAt: ISODate | null;
  doneAt: ISODate | null;
  createdBy: ID | null;
  createdAt: ISODate;
}

export const TASK_STATUSES = ["backlog", "todo", "in_progress", "review", "done"] as const;
export type TaskStatus = (typeof TASK_STATUSES)[number];

export const PRIORITIES = ["low", "medium", "high", "urgent"] as const;
export type Priority = (typeof PRIORITIES)[number];

export interface Project {
  id: ID;
  name: string;
  key: string;
  description: string | null;
  status: "active" | "paused" | "done";
  color: string;
  dueDate: ISODate | null;
  createdBy: ID | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export interface Task {
  id: ID;
  projectId: ID | null;
  title: string;
  description: string | null;
  status: TaskStatus;
  priority: Priority;
  position: number;
  assigneeId: ID | null;
  dueDate: ISODate | null;
  messageId: ID | null;
  createdBy: ID | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export interface Document {
  id: ID;
  title: string;
  slug: string;
  body: string;
  parentId: ID | null;
  createdBy: ID | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export interface CalendarEvent {
  id: ID;
  title: string;
  description: string | null;
  startsAt: ISODate;
  endsAt: ISODate;
  allDay: boolean;
  location: string | null;
  color: string;
  createdBy: ID | null;
  createdAt: ISODate;
  updatedAt: ISODate;
}

export const INVOICE_STATUSES = ["draft", "sent", "paid", "overdue", "void"] as const;
export type InvoiceStatus = (typeof INVOICE_STATUSES)[number];

export interface InvoiceItem {
  id: ID;
  invoiceId: ID;
  description: string;
  quantity: number;
  unitPrice: number;
  amount: number;
  position: number;
}

export interface Invoice {
  id: ID;
  number: string;
  companyId: ID | null;
  contactId: ID | null;
  status: InvoiceStatus;
  issueDate: ISODate;
  dueDate: ISODate;
  currency: string;
  subtotal: number;
  taxRate: number;
  total: number;
  notes: string | null;
  createdBy: ID | null;
  createdAt: ISODate;
  updatedAt: ISODate;
  items?: InvoiceItem[];
}

export const CHANNEL_KINDS = ["channel", "dm"] as const;
export type ChannelKind = (typeof CHANNEL_KINDS)[number];

export interface Channel {
  id: ID;
  kind: ChannelKind;
  name: string;
  topic: string | null;
  isPrivate: boolean;
  createdBy: ID | null;
  createdAt: ISODate;
}

export interface Message {
  id: ID;
  channelId: ID;
  threadId: ID | null;
  authorType: "user" | "agent" | "system";
  authorId: ID | null;
  body: string;
  meta: Record<string, unknown> | null;
  createdAt: ISODate;
}

export interface Tag {
  id: ID;
  name: string;
  color: string;
  createdAt: ISODate;
}

export interface AgentMember {
  id: ID;
  name: string;
  kind: "openclaw" | "open-tag" | "builtin";
  status: "idle" | "working" | "offline";
  config: Record<string, unknown>;
  createdAt: ISODate;
}

export interface ApiKey {
  id: ID;
  name: string;
  prefix: string;
  scopes: string[];
  createdBy: ID | null;
  createdAt: ISODate;
  lastUsedAt: ISODate | null;
  revokedAt: ISODate | null;
}

export interface AuditEntry {
  id: ID;
  actorType: "user" | "agent" | "system";
  actorId: ID | null;
  action: string;
  entityType: string;
  entityId: ID | null;
  meta: Record<string, unknown> | null;
  createdAt: ISODate;
}

export interface SearchHit {
  entityType: string;
  entityId: ID;
  title: string;
  snippet: string;
  score: number;
}

// ---------------------------------------------------------------------------
// Validation schemas (server-side input validation, shared with the client)
// ---------------------------------------------------------------------------

export const emailSchema = z.string().trim().toLowerCase().email();

export const registerSchema = z.object({
  email: emailSchema,
  name: z.string().trim().min(1).max(120),
  password: z.string().min(8).max(200),
  workspaceName: z.string().trim().min(1).max(120).optional(),
});

export const loginSchema = z.object({
  email: emailSchema,
  password: z.string().min(1).max(200),
});

export const contactSchema = z.object({
  firstName: z.string().trim().min(1).max(80),
  lastName: z.string().trim().min(1).max(80),
  email: emailSchema.optional().nullable(),
  phone: z.string().trim().max(40).optional().nullable(),
  title: z.string().trim().max(120).optional().nullable(),
  companyId: z.string().optional().nullable(),
  notes: z.string().max(10000).optional().nullable(),
});

export const companySchema = z.object({
  name: z.string().trim().min(1).max(160),
  domain: z.string().trim().max(160).optional().nullable(),
  industry: z.string().trim().max(80).optional().nullable(),
  size: z.string().trim().max(40).optional().nullable(),
  website: z.string().trim().max(300).optional().nullable(),
  notes: z.string().max(10000).optional().nullable(),
});

export const dealSchema = z.object({
  name: z.string().trim().min(1).max(160),
  companyId: z.string().optional().nullable(),
  contactId: z.string().optional().nullable(),
  stage: z.enum(DEAL_STAGES).default("lead"),
  value: z.number().min(0).default(0),
  currency: z.string().trim().length(3).default("USD"),
  closeDate: z.string().optional().nullable(),
  ownerId: z.string().optional().nullable(),
  notes: z.string().max(10000).optional().nullable(),
});

export const taskSchema = z.object({
  projectId: z.string().optional().nullable(),
  title: z.string().trim().min(1).max(240),
  description: z.string().max(20000).optional().nullable(),
  status: z.enum(TASK_STATUSES).default("todo"),
  priority: z.enum(PRIORITIES).default("medium"),
  position: z.number().optional(),
  assigneeId: z.string().optional().nullable(),
  dueDate: z.string().optional().nullable(),
});

export const projectSchema = z.object({
  name: z.string().trim().min(1).max(160),
  key: z.string().trim().min(2).max(12).optional(),
  description: z.string().max(20000).optional().nullable(),
  status: z.enum(["active", "paused", "done"]).default("active"),
  color: z.string().trim().max(20).optional(),
  dueDate: z.string().optional().nullable(),
});

export const documentSchema = z.object({
  title: z.string().trim().min(1).max(240),
  body: z.string().max(500000).default(""),
  parentId: z.string().optional().nullable(),
});

export const eventSchema = z.object({
  title: z.string().trim().min(1).max(240),
  description: z.string().max(20000).optional().nullable(),
  startsAt: z.string(),
  endsAt: z.string(),
  allDay: z.boolean().default(false),
  location: z.string().trim().max(240).optional().nullable(),
  color: z.string().trim().max(20).optional(),
});

export const invoiceItemSchema = z.object({
  description: z.string().trim().min(1).max(240),
  quantity: z.number().min(0),
  unitPrice: z.number(),
});

export const invoiceSchema = z.object({
  companyId: z.string().optional().nullable(),
  contactId: z.string().optional().nullable(),
  status: z.enum(INVOICE_STATUSES).default("draft"),
  issueDate: z.string(),
  dueDate: z.string(),
  currency: z.string().trim().length(3).default("USD"),
  taxRate: z.number().min(0).max(1).default(0),
  notes: z.string().max(20000).optional().nullable(),
  items: z.array(invoiceItemSchema).default([]),
});

export const messageSchema = z.object({
  body: z.string().trim().min(1).max(20000),
  threadId: z.string().optional().nullable(),
});

export const channelSchema = z.object({
  kind: z.enum(CHANNEL_KINDS).default("channel"),
  name: z.string().trim().min(1).max(80),
  topic: z.string().trim().max(240).optional().nullable(),
  isPrivate: z.boolean().default(false),
});
