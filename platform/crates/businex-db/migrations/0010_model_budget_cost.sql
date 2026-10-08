-- Monetary cost is reserved alongside tokens so concurrent priced calls
-- cannot all pass the same remaining balance. The reservation row carries
-- the cost estimate that was held.
ALTER TABLE model_budgets ADD COLUMN IF NOT EXISTS reserved_cost_micros BIGINT NOT NULL DEFAULT 0 CHECK (reserved_cost_micros >= 0);
ALTER TABLE model_reservations ADD COLUMN IF NOT EXISTS cost_estimate_micros BIGINT CHECK (cost_estimate_micros IS NULL OR cost_estimate_micros >= 0);

-- Observed state is kept separate from the conservative budget charge:
-- settled_tokens is what the cap sees (observed usage when known, the
-- reserved estimate when unknown), while observed_tokens and the unknown
-- counters record what was actually reported.
ALTER TABLE model_budgets ADD COLUMN IF NOT EXISTS observed_tokens BIGINT NOT NULL DEFAULT 0 CHECK (observed_tokens >= 0);
ALTER TABLE model_budgets ADD COLUMN IF NOT EXISTS unknown_usage_calls BIGINT NOT NULL DEFAULT 0 CHECK (unknown_usage_calls >= 0);
