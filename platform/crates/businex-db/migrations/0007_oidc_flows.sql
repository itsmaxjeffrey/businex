-- OIDC login support.
--
-- oidc_flows: pending authorization-code flows. Each browser round trip is
-- bound to (a) a single-use CSRF state hash and (b) a browser-binding hash
-- (sha256 of an HttpOnly cookie value set at /start). The callback must
-- present the same browser before the flow row is consumed, so a stolen
-- state/code pair cannot be redeemed from another browser (login CSRF).
-- Consuming uses DELETE ... RETURNING so a replayed state is refused even
-- across process restarts and concurrent requests.
--
-- oidc_identities: subject mapping keyed by (issuer, subject) so a provider
-- configuration change (different issuer, same subject string) can never
-- attach to an account provisioned under another issuer.

CREATE TABLE IF NOT EXISTS oidc_flows (
  state_hash    TEXT PRIMARY KEY,
  browser_hash  TEXT NOT NULL,
  nonce         TEXT NOT NULL,
  code_verifier TEXT NOT NULL,
  return_to     TEXT NOT NULL DEFAULT '/',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at    TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS oidc_identities (
  issuer     TEXT NOT NULL,
  subject    TEXT NOT NULL,
  user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (issuer, subject)
);

CREATE INDEX IF NOT EXISTS idx_oidc_identities_user ON oidc_identities (user_id);

-- The legacy subject column is replaced by the composite mapping above.
ALTER TABLE users DROP COLUMN IF EXISTS oidc_subject;

GRANT SELECT, INSERT, DELETE ON oidc_flows TO businex_app, businex_service;
GRANT SELECT, INSERT, DELETE ON oidc_identities TO businex_app, businex_service;
