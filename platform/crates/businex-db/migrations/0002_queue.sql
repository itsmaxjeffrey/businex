-- Durable job queue, schedules and external-effect tracking.
-- Durable work lives here in PostgreSQL. Redis Pub/Sub never carries it.

CREATE TABLE IF NOT EXISTS jobs (
  id              UUID PRIMARY KEY,
  company_id      UUID REFERENCES companies(id) ON DELETE CASCADE,
  kind            TEXT NOT NULL,
  payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
  idempotency_key TEXT,
  state           TEXT NOT NULL DEFAULT 'queued'
                  CHECK (state IN ('queued','leased','succeeded','failed','canceled')),
  attempts        INTEGER NOT NULL DEFAULT 0,
  max_attempts    INTEGER NOT NULL DEFAULT 5,
  priority        INTEGER NOT NULL DEFAULT 0,
  available_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  lease_owner     TEXT,
  lease_expires_at TIMESTAMPTZ,
  canceled_at     TIMESTAMPTZ,
  last_error      TEXT,
  result          JSONB,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One live job per (company, idempotency key): duplicate enqueue is a no-op.
CREATE UNIQUE INDEX IF NOT EXISTS jobs_idempotency
  ON jobs (company_id, idempotency_key)
  WHERE idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS jobs_claimable
  ON jobs (state, available_at, priority DESC, created_at);

CREATE INDEX IF NOT EXISTS jobs_company_created
  ON jobs (company_id, created_at DESC);

-- External effects (webhooks, provider calls) are recorded before the call and
-- resolved after it, so a worker restart can reconcile instead of repeating an
-- uncertain external action.
CREATE TABLE IF NOT EXISTS job_effects (
  id           UUID PRIMARY KEY,
  job_id       UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  effect_key   TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  outcome      TEXT NOT NULL DEFAULT 'pending'
               CHECK (outcome IN ('pending','confirmed','failed')),
  detail       JSONB,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  resolved_at  TIMESTAMPTZ,
  UNIQUE (job_id, effect_key)
);

CREATE TABLE IF NOT EXISTS job_schedules (
  id               UUID PRIMARY KEY,
  company_id       UUID REFERENCES companies(id) ON DELETE CASCADE,
  kind             TEXT NOT NULL,
  payload          JSONB NOT NULL DEFAULT '{}'::jsonb,
  interval_seconds INTEGER CHECK (interval_seconds IS NULL OR interval_seconds > 0),
  cron             TEXT,
  next_run_at      TIMESTAMPTZ NOT NULL,
  enabled          BOOLEAN NOT NULL DEFAULT true,
  last_job_id      UUID,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK (interval_seconds IS NOT NULL OR cron IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS job_schedules_due
  ON job_schedules (enabled, next_run_at);

CREATE TABLE IF NOT EXISTS job_events (
  id         BIGSERIAL PRIMARY KEY,
  job_id     UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  from_state TEXT,
  to_state   TEXT NOT NULL,
  detail     JSONB,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_job_events_job ON job_events (job_id, created_at);

ALTER TABLE jobs          ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_effects   ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_schedules ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_events    ENABLE ROW LEVEL SECURITY;

ALTER TABLE jobs          FORCE ROW LEVEL SECURITY;
ALTER TABLE job_effects   FORCE ROW LEVEL SECURITY;
ALTER TABLE job_schedules FORCE ROW LEVEL SECURITY;
ALTER TABLE job_events    FORCE ROW LEVEL SECURITY;

CREATE POLICY jobs_company_isolation ON jobs
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

CREATE POLICY job_schedules_company_isolation ON job_schedules
  USING (company_id = nullif(current_setting('businex.company_id', true), '')::uuid)
  WITH CHECK (company_id = nullif(current_setting('businex.company_id', true), '')::uuid);

-- Effects and events follow their job's company.
CREATE POLICY job_effects_company_isolation ON job_effects
  USING (EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = job_effects.job_id
      AND j.company_id = nullif(current_setting('businex.company_id', true), '')::uuid))
  WITH CHECK (EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = job_effects.job_id
      AND j.company_id = nullif(current_setting('businex.company_id', true), '')::uuid));

CREATE POLICY job_events_company_isolation ON job_events
  USING (EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = job_events.job_id
      AND j.company_id = nullif(current_setting('businex.company_id', true), '')::uuid))
  WITH CHECK (EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = job_events.job_id
      AND j.company_id = nullif(current_setting('businex.company_id', true), '')::uuid));

GRANT SELECT, INSERT, UPDATE, DELETE ON
  jobs, job_effects, job_schedules, job_events, job_events_id_seq
  TO businex_app, businex_service;
