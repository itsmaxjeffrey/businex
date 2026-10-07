-- Role provisioning for Businex PostgreSQL.
--
-- Run this as a cluster administrator (for example the managed-service admin
-- user) before first start, and after any role drift. It is idempotent.
--
-- Role contract enforced at API/worker startup (fail closed):
--   businex_app     LOGIN role for the API runtime. NOT superuser, NOT
--                   BYPASSRLS, NOT a member of businex_service. Row level
--                   security confines it to one tenant per transaction.
--   businex_service LOGIN role for workers and maintenance. NOT superuser,
--                   BYPASSRLS so jobs for any tenant can be processed.
--
-- Passwords are set out of band from a secret store; replace the placeholders
-- and never commit the filled-in file.

BEGIN;

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

ALTER ROLE businex_app NOBYPASSRLS NOSUPERUSER NOCREATEDB NOCREATEROLE;
ALTER ROLE businex_service BYPASSRLS NOSUPERUSER NOCREATEDB NOCREATEROLE;

GRANT USAGE ON SCHEMA public TO businex_app, businex_service;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO businex_app, businex_service;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO businex_app, businex_service;

-- Enable login and set passwords from your secret store, for example:
--   ALTER ROLE businex_app LOGIN PASSWORD '...from secret store...';
--   ALTER ROLE businex_service LOGIN PASSWORD '...from secret store...';

COMMIT;
