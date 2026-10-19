ALTER TABLE indexing_metadata
    ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT TRUE;

CREATE UNIQUE INDEX IF NOT EXISTS idx_event_stats_contract_name
    ON event_statistics (contract_address, event_name);

CREATE TABLE IF NOT EXISTS indexing_backfill_checkpoints (
    contract_address VARCHAR(64) NOT NULL,
    from_block BIGINT NOT NULL,
    to_block BIGINT NOT NULL,
    last_indexed_block BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (contract_address, from_block, to_block),
    CHECK (from_block <= to_block),
    CHECK (last_indexed_block >= from_block - 1 AND last_indexed_block <= to_block)
);
