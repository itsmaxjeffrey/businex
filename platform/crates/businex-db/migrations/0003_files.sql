-- File and artifact metadata. Bytes live in S3-compatible storage; this table
-- is the tenant-scoped index that authorization checks before any transfer.

CREATE TABLE IF NOT EXISTS files (
  id           UUID PRIMARY KEY,
  company_id   UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  bucket       TEXT NOT NULL,
  object_key   TEXT NOT NULL,
  name         TEXT NOT NULL,
  content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
  size_bytes   BIGINT NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
  sha256       TEXT,
  created_by   UUID,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at   TIMESTAMPTZ,
  UNIQUE (company_id, bucket, object_key)
);

CREATE INDEX IF NOT EXISTS idx_files_company_created ON files (company_id, created_at DESC);

ALTER TABLE files ENABLE ROW LEVEL SECURITY;
ALTER TABLE files FORCE ROW LEVEL SECURITY;

CREATE POLICY files_company_isolation ON files
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON files TO businex_app, businex_service;
