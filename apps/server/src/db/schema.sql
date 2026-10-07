-- Businex schema. Idempotent: every statement uses IF NOT EXISTS.
-- Applied in order by the migration runner (src/db/migrate.ts).

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
  id            TEXT PRIMARY KEY,
  email         TEXT NOT NULL UNIQUE,
  name          TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  avatar_color  TEXT NOT NULL DEFAULT '#6366f1',
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  last_seen_at  TEXT
);

CREATE TABLE IF NOT EXISTS organizations (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  slug       TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspaces (
  id         TEXT PRIMARY KEY,
  org_id     TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  slug       TEXT NOT NULL UNIQUE,
  settings   TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memberships (
  id           TEXT PRIMARY KEY,
  user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  role         TEXT NOT NULL DEFAULT 'member',
  created_at   TEXT NOT NULL,
  UNIQUE (user_id, workspace_id)
);

CREATE TABLE IF NOT EXISTS teams (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
  team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  PRIMARY KEY (team_id, user_id)
);

CREATE TABLE IF NOT EXISTS invitations (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  email        TEXT NOT NULL,
  role         TEXT NOT NULL DEFAULT 'member',
  token        TEXT NOT NULL UNIQUE,
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  accepted_at  TEXT
);

CREATE TABLE IF NOT EXISTS sessions (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  user_agent TEXT,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  prefix       TEXT NOT NULL,
  key_hash     TEXT NOT NULL UNIQUE,
  scopes       TEXT NOT NULL DEFAULT '[]',
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  last_used_at TEXT,
  revoked_at   TEXT
);

CREATE TABLE IF NOT EXISTS audit_log (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  actor_type   TEXT NOT NULL DEFAULT 'user',
  actor_id     TEXT,
  action       TEXT NOT NULL,
  entity_type  TEXT NOT NULL,
  entity_id    TEXT,
  meta         TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_ws_created ON audit_log (workspace_id, created_at DESC);

-- CRM -------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS companies (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  domain       TEXT,
  industry     TEXT,
  size         TEXT,
  website      TEXT,
  notes        TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_companies_ws ON companies (workspace_id, name);

CREATE TABLE IF NOT EXISTS contacts (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  first_name   TEXT NOT NULL,
  last_name    TEXT NOT NULL,
  email        TEXT,
  phone        TEXT,
  title        TEXT,
  company_id   TEXT REFERENCES companies(id) ON DELETE SET NULL,
  notes        TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_contacts_ws ON contacts (workspace_id, last_name, first_name);

CREATE TABLE IF NOT EXISTS deals (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  company_id   TEXT REFERENCES companies(id) ON DELETE SET NULL,
  contact_id   TEXT REFERENCES contacts(id) ON DELETE SET NULL,
  stage        TEXT NOT NULL DEFAULT 'lead',
  value        REAL NOT NULL DEFAULT 0,
  currency     TEXT NOT NULL DEFAULT 'USD',
  close_date   TEXT,
  owner_id     TEXT,
  notes        TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_deals_ws_stage ON deals (workspace_id, stage);

CREATE TABLE IF NOT EXISTS activities (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  type         TEXT NOT NULL DEFAULT 'note',
  subject      TEXT NOT NULL,
  body         TEXT,
  entity_type  TEXT NOT NULL,
  entity_id    TEXT NOT NULL,
  due_at       TEXT,
  done_at      TEXT,
  created_by   TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_activities_entity ON activities (workspace_id, entity_type, entity_id);

-- Projects --------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS projects (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  key          TEXT NOT NULL,
  description  TEXT,
  status       TEXT NOT NULL DEFAULT 'active',
  color        TEXT NOT NULL DEFAULT '#6366f1',
  due_date     TEXT,
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_projects_ws ON projects (workspace_id, name);

CREATE TABLE IF NOT EXISTS tasks (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id   TEXT REFERENCES projects(id) ON DELETE SET NULL,
  title        TEXT NOT NULL,
  description  TEXT,
  status       TEXT NOT NULL DEFAULT 'todo',
  priority     TEXT NOT NULL DEFAULT 'medium',
  position     REAL NOT NULL DEFAULT 0,
  assignee_id  TEXT,
  due_date     TEXT,
  message_id   TEXT,
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_ws_status ON tasks (workspace_id, status, position);
CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks (project_id, status, position);

CREATE TABLE IF NOT EXISTS task_comments (
  id         TEXT PRIMARY KEY,
  task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  author_id  TEXT,
  body       TEXT NOT NULL,
  created_at TEXT NOT NULL
);

-- Documents -------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS documents (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  title        TEXT NOT NULL,
  slug         TEXT NOT NULL,
  body         TEXT NOT NULL DEFAULT '',
  parent_id    TEXT REFERENCES documents(id) ON DELETE SET NULL,
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_documents_ws ON documents (workspace_id, title);

-- Calendar --------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS events (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  title        TEXT NOT NULL,
  description  TEXT,
  starts_at    TEXT NOT NULL,
  ends_at      TEXT NOT NULL,
  all_day      INTEGER NOT NULL DEFAULT 0,
  location     TEXT,
  color        TEXT NOT NULL DEFAULT '#6366f1',
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_ws_starts ON events (workspace_id, starts_at);

-- Invoices --------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS invoices (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  number       TEXT NOT NULL,
  company_id   TEXT REFERENCES companies(id) ON DELETE SET NULL,
  contact_id   TEXT REFERENCES contacts(id) ON DELETE SET NULL,
  status       TEXT NOT NULL DEFAULT 'draft',
  issue_date   TEXT NOT NULL,
  due_date     TEXT NOT NULL,
  currency     TEXT NOT NULL DEFAULT 'USD',
  subtotal     REAL NOT NULL DEFAULT 0,
  tax_rate     REAL NOT NULL DEFAULT 0,
  total        REAL NOT NULL DEFAULT 0,
  notes        TEXT,
  created_by   TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  UNIQUE (workspace_id, number)
);

CREATE TABLE IF NOT EXISTS invoice_items (
  id          TEXT PRIMARY KEY,
  invoice_id  TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
  description TEXT NOT NULL,
  quantity    REAL NOT NULL DEFAULT 1,
  unit_price  REAL NOT NULL DEFAULT 0,
  amount      REAL NOT NULL DEFAULT 0,
  position    INTEGER NOT NULL DEFAULT 0
);

-- Channels (open-tag model) ---------------------------------------------------

CREATE TABLE IF NOT EXISTS channels (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  kind         TEXT NOT NULL DEFAULT 'channel',
  name         TEXT NOT NULL,
  topic        TEXT,
  is_private   INTEGER NOT NULL DEFAULT 0,
  created_by   TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_channels_ws ON channels (workspace_id, kind, name);

CREATE TABLE IF NOT EXISTS channel_members (
  channel_id   TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  user_id      TEXT NOT NULL,
  role         TEXT NOT NULL DEFAULT 'member',
  last_read_at TEXT,
  joined_at    TEXT NOT NULL,
  PRIMARY KEY (channel_id, user_id)
);

CREATE TABLE IF NOT EXISTS messages (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  channel_id   TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
  thread_id    TEXT,
  author_type  TEXT NOT NULL DEFAULT 'user',
  author_id    TEXT,
  body         TEXT NOT NULL,
  meta         TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages (channel_id, created_at);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages (thread_id, created_at);

CREATE TABLE IF NOT EXISTS attachments (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  message_id   TEXT REFERENCES messages(id) ON DELETE CASCADE,
  filename     TEXT NOT NULL,
  mime         TEXT NOT NULL DEFAULT 'application/octet-stream',
  size         INTEGER NOT NULL DEFAULT 0,
  path         TEXT NOT NULL,
  created_at   TEXT NOT NULL
);

-- Agents (OpenClaw / open-tag) ------------------------------------------------

CREATE TABLE IF NOT EXISTS agents (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  kind         TEXT NOT NULL DEFAULT 'openclaw',
  status       TEXT NOT NULL DEFAULT 'idle',
  config       TEXT NOT NULL DEFAULT '{}',
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_sessions (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  agent_id     TEXT REFERENCES agents(id) ON DELETE CASCADE,
  external_ref TEXT,
  title        TEXT,
  status       TEXT NOT NULL DEFAULT 'idle',
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_events (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  agent_id     TEXT,
  kind         TEXT NOT NULL,
  payload      TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_agent_events_ws ON agent_events (workspace_id, created_at DESC);

CREATE TABLE IF NOT EXISTS integration_settings (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  provider     TEXT NOT NULL,
  config       TEXT NOT NULL DEFAULT '{}',
  updated_at   TEXT NOT NULL,
  UNIQUE (workspace_id, provider)
);

-- Tags & search ---------------------------------------------------------------

CREATE TABLE IF NOT EXISTS tags (
  id           TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  color        TEXT NOT NULL DEFAULT '#64748b',
  created_at   TEXT NOT NULL,
  UNIQUE (workspace_id, name)
);

CREATE TABLE IF NOT EXISTS entity_tags (
  workspace_id TEXT NOT NULL,
  entity_type  TEXT NOT NULL,
  entity_id    TEXT NOT NULL,
  tag_id       TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (entity_type, entity_id, tag_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
  entity_type, entity_id, workspace_id UNINDEXED, title, body,
  tokenize = 'unicode61 remove_diacritics 2'
);

-- User preferences ------------------------------------------------------------

CREATE TABLE IF NOT EXISTS user_prefs (
  user_id      TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  prefs        TEXT NOT NULL DEFAULT '{}',
  updated_at   TEXT NOT NULL,
  PRIMARY KEY (user_id, workspace_id)
);
