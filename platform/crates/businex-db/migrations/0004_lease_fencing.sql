-- Lease fencing and role-attribute hardening.
--
-- Queue leases get a fencing token: every claim records a new token and
-- completion, failure and heartbeat must present the matching token with an
-- unexpired lease. This fences stale attempts, including a stale attempt whose
-- worker id matches the current one.

ALTER TABLE jobs ADD COLUMN IF NOT EXISTS lease_token UUID;

-- Re-assert role attributes for databases created before 0004. On managed
-- services ALTER ROLE may be forbidden: that is surfaced as a warning and must
-- be handled by the documented provisioning path (deploy/provision-roles.sql).
-- Enforcement is fail-closed at API/worker startup via
-- businex_db::current_role_attributes + ensure_*_role_safe, which read the
-- real catalog attributes instead of trusting this migration.
DO $$
BEGIN
  BEGIN
    ALTER ROLE businex_service BYPASSRLS;
  EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'could not ALTER ROLE businex_service: %; provision via deploy/provision-roles.sql', SQLERRM;
  END;
  BEGIN
    ALTER ROLE businex_app NOBYPASSRLS NOSUPERUSER;
  EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'could not ALTER ROLE businex_app: %; provision via deploy/provision-roles.sql', SQLERRM;
  END;
END
$$;
