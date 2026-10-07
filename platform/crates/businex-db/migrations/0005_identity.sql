-- Identity support functions.
--
-- Tenant bootstrap and identity lookups cross the RLS boundary by design (a
-- user can belong to several companies; company creation must write both the
-- tenant row and its owner membership). These SECURITY DEFINER functions are
-- the ONLY sanctioned cross-tenant access for the runtime role: each one does
-- exactly one narrowly scoped operation. Everything else stays behind RLS.

CREATE SCHEMA IF NOT EXISTS businex;
GRANT USAGE ON SCHEMA businex TO businex_app, businex_service;

-- Memberships of one user (login landing list).
CREATE OR REPLACE FUNCTION businex.my_memberships(p_user_id uuid)
RETURNS TABLE (company_id uuid, role text, scope jsonb)
LANGUAGE sql SECURITY DEFINER SET search_path = public STABLE
AS $$
  SELECT m.company_id, m.role, m.scope FROM memberships m WHERE m.user_id = p_user_id
$$;

-- One membership record: the basis of every per-request authorization.
CREATE OR REPLACE FUNCTION businex.membership_for(p_user_id uuid, p_company_id uuid)
RETURNS TABLE (role text, scope jsonb)
LANGUAGE sql SECURITY DEFINER SET search_path = public STABLE
AS $$
  SELECT m.role, m.scope FROM memberships m
  WHERE m.user_id = p_user_id AND m.company_id = p_company_id
$$;

-- Create a company and its owner membership atomically.
CREATE OR REPLACE FUNCTION businex.create_company(p_name text, p_slug text, p_user_id uuid)
RETURNS uuid
LANGUAGE plpgsql SECURITY DEFINER SET search_path = public
AS $$
DECLARE
  v_id uuid := gen_random_uuid();
BEGIN
  INSERT INTO companies (id, name, slug) VALUES (v_id, p_name, p_slug);
  INSERT INTO memberships (id, company_id, user_id, role)
  VALUES (gen_random_uuid(), v_id, p_user_id, 'owner');
  RETURN v_id;
END
$$;

-- Look up an invitation by its token hash (accept flows cross tenants by
-- nature: the token is the authorization).
CREATE OR REPLACE FUNCTION businex.invitation_by_token(p_hash text)
RETURNS TABLE (id uuid, company_id uuid, email text, role text, expires_at timestamptz, accepted_at timestamptz)
LANGUAGE sql SECURITY DEFINER SET search_path = public STABLE
AS $$
  SELECT i.id, i.company_id, i.email, i.role, i.expires_at, i.accepted_at
  FROM invitations i WHERE i.token_hash = p_hash
$$;

REVOKE ALL ON FUNCTION
  businex.my_memberships(uuid),
  businex.membership_for(uuid, uuid),
  businex.create_company(text, text, uuid),
  businex.invitation_by_token(text)
FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
  businex.my_memberships(uuid),
  businex.membership_for(uuid, uuid),
  businex.create_company(text, text, uuid),
  businex.invitation_by_token(text)
TO businex_app;

-- The functions run with the service role's BYPASSRLS attribute so FORCE ROW
-- LEVEL SECURITY cannot strangle them; the app role can only execute the four
-- narrow entry points above.
ALTER FUNCTION businex.my_memberships(uuid) OWNER TO businex_service;
ALTER FUNCTION businex.membership_for(uuid, uuid) OWNER TO businex_service;
ALTER FUNCTION businex.create_company(text, text, uuid) OWNER TO businex_service;
ALTER FUNCTION businex.invitation_by_token(text) OWNER TO businex_service;
