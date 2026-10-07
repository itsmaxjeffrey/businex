-- Lease fencing and role-attribute hardening.
--
-- Queue leases get a fencing token: every claim records a new token and
-- completion, failure and heartbeat must present the matching token with an
-- unexpired lease. This fences stale attempts, including a stale attempt whose
-- worker id matches the current one.

ALTER TABLE jobs ADD COLUMN IF NOT EXISTS lease_token UUID;

-- Re-assert role attributes for databases created before 0004. Errors are
-- ignored on managed services where role attributes are provisioned out of band.
DO $$
BEGIN
  BEGIN
    ALTER ROLE businex_service BYPASSRLS;
  EXCEPTION WHEN OTHERS THEN NULL;
  END;
  BEGIN
    ALTER ROLE businex_app NOBYPASSRLS NOSUPERUSER;
  EXCEPTION WHEN OTHERS THEN NULL;
  END;
END
$$;
