-- StellarLend – initial schema
-- Applied automatically by postgres on first container start.

CREATE TABLE IF NOT EXISTS api_keys (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    prefix        VARCHAR(8)   NOT NULL,
    hash          TEXT         NOT NULL,
    name          VARCHAR(255),
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ,
    created_by    TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys (prefix);

CREATE TABLE IF NOT EXISTS audit_logs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sequence      BIGINT       NOT NULL,
    action        TEXT         NOT NULL,
    actor         TEXT         NOT NULL,
    status        TEXT         NOT NULL,
    tx_hash       TEXT,
    ledger        BIGINT,
    amount        TEXT,
    asset_address TEXT,
    ip            TEXT,
    prev_hash     TEXT         NOT NULL,
    hash          TEXT         NOT NULL,
    timestamp     TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_actor  ON audit_logs (actor);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs (action);
