-- Core identity, tenancy and audit schema with PostgreSQL row level security.
-- Roles: businex_app (application runtime, RLS enforced) and businex_service
-- (workers and maintenance, BYPASSRLS). Both are NOLOGIN here; provisioning
-- grants the login password out of band and never stores it in the repository.

-- Role attributes must match the documented RLS behavior: the runtime role is
-- an ordinary non-superuser subject to RLS; the service role (workers,
-- maintenance) bypasses RLS. Neither role can be assumed by the other.
DO $$
BEGIN
  BEGIN
    CREATE ROLE businex_app NOLOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
  EXCEPTION WHEN duplicate_object THEN NULL;
  END;
  BEGIN
    CREATE ROLE businex_service NOLOGIN NOSUPERUSER BYPASSRLS NOCREATEDB NOCREATEROLE;
  EXCEPTION WHEN duplicate_object THEN NULL;
  END;
END
$$;

GRANT USAGE ON SCHEMA public TO businex_app, businex_service;

-- Global identities (users can belong to several companies).
CREATE TABLE IF NOT EXISTS users (
  id            UUID PRIMARY KEY,
  email         TEXT NOT NULL UNIQUE,
  name          TEXT NOT NULL,
  password_hash TEXT,
  oidc_subject  TEXT UNIQUE,
  avatar_color  TEXT NOT NULL DEFAULT '#6366f1',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at  TIMESTAMPTZ
);

-- Tenants.
CREATE TABLE IF NOT EXISTS companies (
  id         UUID PRIMARY KEY,
  name       TEXT NOT NULL,
  slug       TEXT NOT NULL UNIQUE,
  settings   JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS memberships (
  id         UUID PRIMARY KEY,
  company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role       TEXT NOT NULL DEFAULT 'member'
             CHECK (role IN ('viewer','member','manager','admin','owner')),
  scope      JSONB NOT NULL DEFAULT '{"resources":[]}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (company_id, user_id)
);

CREATE TABLE IF NOT EXISTS invitations (
  id          UUID PRIMARY KEY,
  company_id  UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  email       TEXT NOT NULL,
  role        TEXT NOT NULL DEFAULT 'member'
              CHECK (role IN ('viewer','member','manager','admin','owner')),
  token_hash  TEXT NOT NULL UNIQUE,
  created_by  UUID,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at  TIMESTAMPTZ NOT NULL,
  accepted_at TIMESTAMPTZ
);

-- Server sessions; the token is stored hashed, never in plain text.
CREATE TABLE IF NOT EXISTS sessions (
  id           UUID PRIMARY KEY,
  user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash   TEXT NOT NULL UNIQUE,
  user_agent   TEXT,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at   TIMESTAMPTZ NOT NULL,
  revoked_at   TIMESTAMPTZ
);

-- Scoped action grants for agents and generated apps. A grant can only narrow
-- an existing role permission; it can never widen it.
CREATE TABLE IF NOT EXISTS action_grants (
  id          UUID PRIMARY KEY,
  company_id  UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  actor_type  TEXT NOT NULL CHECK (actor_type IN ('agent','generated_app')),
  actor_id    UUID NOT NULL,
  permission  TEXT NOT NULL,
  resource    TEXT NOT NULL DEFAULT '*',
  granted_by  UUID,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at  TIMESTAMPTZ,
  UNIQUE (company_id, actor_type, actor_id, permission, resource)
);

CREATE TABLE IF NOT EXISTS audit_log (
  id          UUID PRIMARY KEY,
  company_id  UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  actor_type  TEXT NOT NULL DEFAULT 'user',
  actor_id    UUID,
  action      TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id   TEXT,
  meta        JSONB,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_audit_company_created ON audit_log (company_id, created_at DESC);

-- Row level security: every tenant table is filtered by the company id set on
-- the transaction (businex.company_id). An unset context matches nothing.
ALTER TABLE companies   ENABLE ROW LEVEL SECURITY;
ALTER TABLE memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE invitations ENABLE ROW LEVEL SECURITY;
ALTER TABLE action_grants ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log   ENABLE ROW LEVEL SECURITY;

ALTER TABLE companies   FORCE ROW LEVEL SECURITY;
ALTER TABLE memberships FORCE ROW LEVEL SECURITY;
ALTER TABLE invitations FORCE ROW LEVEL SECURITY;
ALTER TABLE action_grants FORCE ROW LEVEL SECURITY;
ALTER TABLE audit_log   FORCE ROW LEVEL SECURITY;

CREATE POLICY companies_company_isolation ON companies
  USING (id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY memberships_company_isolation ON memberships
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY invitations_company_isolation ON invitations
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY action_grants_company_isolation ON action_grants
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY audit_log_company_isolation ON audit_log
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON
  users, companies, memberships, invitations, sessions, action_grants, audit_log
  TO businex_app, businex_service;
