-- Model keys, usage accounting and durable budgets.
--
-- The in-memory ledger in businex-models is a unit-test helper only. Production
-- reservations and usage live here so API and worker processes/replicas share
-- one atomic view: restarting a process must not reset spending or caps.

CREATE TABLE IF NOT EXISTS model_keys (
  id            UUID PRIMARY KEY,
  company_id    UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  provider      TEXT NOT NULL,
  label         TEXT NOT NULL DEFAULT '',
  sealed_nonce  BYTEA NOT NULL,
  sealed_bytes  BYTEA NOT NULL,
  created_by    UUID,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  revoked_at    TIMESTAMPTZ,
  UNIQUE (company_id, provider, label)
);

CREATE TABLE IF NOT EXISTS model_budgets (
  company_id              UUID PRIMARY KEY REFERENCES companies(id) ON DELETE CASCADE,
  max_tokens_per_period   BIGINT CHECK (max_tokens_per_period IS NULL OR max_tokens_per_period > 0),
  max_cost_micros         BIGINT CHECK (max_cost_micros IS NULL OR max_cost_micros >= 0),
  deny_when_cost_unknown  BOOLEAN NOT NULL DEFAULT false,
  reserved_tokens         BIGINT NOT NULL DEFAULT 0 CHECK (reserved_tokens >= 0),
  settled_tokens          BIGINT NOT NULL DEFAULT 0 CHECK (settled_tokens >= 0),
  settled_cost_micros     BIGINT NOT NULL DEFAULT 0 CHECK (settled_cost_micros >= 0),
  unknown_cost_calls      BIGINT NOT NULL DEFAULT 0 CHECK (unknown_cost_calls >= 0),
  updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_reservations (
  id          UUID PRIMARY KEY,
  company_id  UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  tokens      BIGINT NOT NULL CHECK (tokens >= 0),
  state       TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open','settled','released')),
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  settled_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_model_reservations_open
  ON model_reservations (company_id) WHERE state = 'open';

CREATE TABLE IF NOT EXISTS model_usage (
  id                 UUID PRIMARY KEY,
  company_id         UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
  provider           TEXT NOT NULL,
  model              TEXT NOT NULL,
  input_tokens       BIGINT,
  output_tokens      BIGINT,
  cost_micros        BIGINT,
  cost_known         BOOLEAN NOT NULL DEFAULT false,
  reservation_id     UUID REFERENCES model_reservations(id),
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_model_usage_company
  ON model_usage (company_id, created_at DESC);

ALTER TABLE model_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_budgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_reservations ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_usage ENABLE ROW LEVEL SECURITY;

ALTER TABLE model_keys FORCE ROW LEVEL SECURITY;
ALTER TABLE model_budgets FORCE ROW LEVEL SECURITY;
ALTER TABLE model_reservations FORCE ROW LEVEL SECURITY;
ALTER TABLE model_usage FORCE ROW LEVEL SECURITY;

CREATE POLICY model_keys_company_isolation ON model_keys
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY model_budgets_company_isolation ON model_budgets
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY model_reservations_company_isolation ON model_reservations
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY model_usage_company_isolation ON model_usage
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON
  model_keys, model_budgets, model_reservations, model_usage
  TO businex_app, businex_service;
