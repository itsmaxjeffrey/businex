-- Trusted provider endpoints live with the key, not with the request.
-- Generation requests can no longer supply an endpoint: the stored one is
-- the only address a company key is ever sent to.
ALTER TABLE model_keys ADD COLUMN IF NOT EXISTS endpoint TEXT;

-- How a usage row ended. "observed" means a response with usage arrived;
-- "ambiguous" means the call was dispatched but its outcome is unknown
-- (timeout, transport failure) and the charge needs reconciliation.
ALTER TABLE model_usage ADD COLUMN IF NOT EXISTS outcome TEXT NOT NULL DEFAULT 'observed';
