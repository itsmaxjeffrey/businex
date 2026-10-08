-- App platform: company app libraries, immutable manifest versions, the
-- install lifecycle and schema-driven records.
--
-- Apps live in a company-scoped library: an admin publishes versioned
-- manifests, installs/activates a version, and upgrades or rolls back with
-- every transition recorded. Uninstall flips status and keeps app_records
-- rows exactly where they are — removing an app never silently deletes data.
--
-- app_versions rows are immutable by trigger: a published manifest and its
-- bundle hash can never be rewritten, only superseded by a new version.

CREATE TABLE IF NOT EXISTS apps (
  id         UUID PRIMARY KEY,
  company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  slug       TEXT NOT NULL CHECK (slug ~ '^[a-z][a-z0-9-]{0,62}$'),
  name       TEXT NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('schema', 'code')),
  created_by UUID,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (company_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_apps_company ON apps (company_id, slug);

CREATE TABLE IF NOT EXISTS app_versions (
  id          UUID PRIMARY KEY,
  app_id      UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
  company_id  UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  version     INTEGER NOT NULL CHECK (version > 0),
  manifest    JSONB NOT NULL,
  bundle_hash TEXT NOT NULL,
  created_by  UUID,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (app_id, version)
);

CREATE OR REPLACE FUNCTION reject_app_version_mutation() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'app_versions rows are immutable';
END
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS app_versions_immutable ON app_versions;
CREATE TRIGGER app_versions_immutable
  BEFORE UPDATE ON app_versions
  FOR EACH ROW EXECUTE FUNCTION reject_app_version_mutation();

CREATE TABLE IF NOT EXISTS app_installs (
  id           UUID PRIMARY KEY,
  company_id   UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  app_id       UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
  version_id   UUID NOT NULL REFERENCES app_versions(id),
  status       TEXT NOT NULL DEFAULT 'active'
               CHECK (status IN ('active', 'disabled', 'uninstalled')),
  installed_by UUID,
  installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (company_id, app_id)
);

CREATE TABLE IF NOT EXISTS app_install_history (
  id           UUID PRIMARY KEY,
  company_id   UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  install_id   UUID NOT NULL REFERENCES app_installs(id) ON DELETE CASCADE,
  action       TEXT NOT NULL
               CHECK (action IN ('install', 'upgrade', 'rollback', 'uninstall')),
  from_version INTEGER,
  to_version   INTEGER NOT NULL,
  actor_id     UUID,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_app_history_install
  ON app_install_history (install_id, created_at DESC);

CREATE TABLE IF NOT EXISTS app_records (
  id         UUID PRIMARY KEY,
  company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  app_id     UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
  entity     TEXT NOT NULL,
  data       JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_by UUID,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_app_records_lookup
  ON app_records (company_id, app_id, entity, created_at DESC);

-- Tenant isolation: the same two-layer pattern as the rest of the schema.
-- RLS confines every table to the company context, and the API layer checks
-- the membership role on top of it.

ALTER TABLE apps ENABLE ROW LEVEL SECURITY;
ALTER TABLE apps FORCE ROW LEVEL SECURITY;
CREATE POLICY apps_company_isolation ON apps
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

ALTER TABLE app_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY app_versions_company_isolation ON app_versions
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

ALTER TABLE app_installs ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_installs FORCE ROW LEVEL SECURITY;
CREATE POLICY app_installs_company_isolation ON app_installs
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

ALTER TABLE app_install_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_install_history FORCE ROW LEVEL SECURITY;
CREATE POLICY app_history_company_isolation ON app_install_history
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

ALTER TABLE app_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE app_records FORCE ROW LEVEL SECURITY;
CREATE POLICY app_records_company_isolation ON app_records
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON apps TO businex_app, businex_service;
GRANT SELECT, INSERT, UPDATE, DELETE ON app_versions TO businex_app, businex_service;
GRANT SELECT, INSERT, UPDATE, DELETE ON app_installs TO businex_app, businex_service;
GRANT SELECT, INSERT, UPDATE, DELETE ON app_install_history TO businex_app, businex_service;
GRANT SELECT, INSERT, UPDATE, DELETE ON app_records TO businex_app, businex_service;