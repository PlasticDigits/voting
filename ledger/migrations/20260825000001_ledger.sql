-- Voting ledger: registration + CL8Y CW20 / BEP-20 history for registered wallets only.
-- Invariants: docs/LEDGER_INVARIANTS.md

CREATE TABLE IF NOT EXISTS indexer_state (
    key VARCHAR(100) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO indexer_state (key, value) VALUES
    ('last_indexed_height', '0'),
    ('last_indexed_block_hash', ''),
    ('last_indexed_bsc_block', '0'),
    ('indexer_version', '1.0.0')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS voting_registrations (
    chain TEXT NOT NULL CHECK (chain IN ('terra', 'bsc')),
    wallet_address TEXT NOT NULL,
    registered_at_height BIGINT NOT NULL,
    registered_at_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    initial_balance NUMERIC(78, 0) NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    signature_id UUID,
    PRIMARY KEY (chain, wallet_address)
);

CREATE INDEX IF NOT EXISTS voting_registrations_chain_status_idx
    ON voting_registrations (chain, status);

CREATE TABLE IF NOT EXISTS cl8y_cw20_transfers (
    id BIGSERIAL PRIMARY KEY,
    height BIGINT NOT NULL,
    tx_hash TEXT NOT NULL,
    msg_index INT NOT NULL DEFAULT 0,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount NUMERIC(78, 0) NOT NULL,
    action TEXT NOT NULL,
    UNIQUE (tx_hash, msg_index, from_address, to_address, amount)
);

CREATE INDEX IF NOT EXISTS cl8y_cw20_transfers_height_idx ON cl8y_cw20_transfers (height);
CREATE INDEX IF NOT EXISTS cl8y_cw20_transfers_from_idx ON cl8y_cw20_transfers (from_address, height);
CREATE INDEX IF NOT EXISTS cl8y_cw20_transfers_to_idx ON cl8y_cw20_transfers (to_address, height);

CREATE TABLE IF NOT EXISTS cl8y_balances (
    wallet_address TEXT NOT NULL,
    height BIGINT NOT NULL,
    balance NUMERIC(78, 0) NOT NULL,
    PRIMARY KEY (wallet_address, height)
);

CREATE TABLE IF NOT EXISTS cl8y_bep20_transfers (
    id BIGSERIAL PRIMARY KEY,
    bsc_block BIGINT NOT NULL,
    tx_hash TEXT NOT NULL,
    log_index INT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount NUMERIC(78, 0) NOT NULL,
    UNIQUE (tx_hash, log_index)
);

CREATE INDEX IF NOT EXISTS cl8y_bep20_transfers_block_idx ON cl8y_bep20_transfers (bsc_block);
CREATE INDEX IF NOT EXISTS cl8y_bep20_transfers_from_idx ON cl8y_bep20_transfers (from_address, bsc_block);
CREATE INDEX IF NOT EXISTS cl8y_bep20_transfers_to_idx ON cl8y_bep20_transfers (to_address, bsc_block);

CREATE TABLE IF NOT EXISTS cl8y_bsc_balances (
    wallet_address TEXT NOT NULL,
    bsc_block BIGINT NOT NULL,
    balance NUMERIC(78, 0) NOT NULL,
    PRIMARY KEY (wallet_address, bsc_block)
);

CREATE OR REPLACE FUNCTION voting_cl8y_balance_at(p_wallet TEXT, p_height BIGINT)
RETURNS NUMERIC
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    r voting_registrations%ROWTYPE;
    v NUMERIC(78, 0);
BEGIN
    SELECT * INTO r
    FROM voting_registrations
    WHERE chain = 'terra' AND wallet_address = lower(trim(p_wallet));
    IF NOT FOUND THEN
        RETURN 0;
    END IF;
    IF p_height < r.registered_at_height THEN
        RETURN 0;
    END IF;
    SELECT balance INTO v
    FROM cl8y_balances
    WHERE wallet_address = r.wallet_address AND height <= p_height
    ORDER BY height DESC
    LIMIT 1;
    IF FOUND THEN
        RETURN v;
    END IF;
    RETURN r.initial_balance;
END;
$$;

ALTER FUNCTION voting_cl8y_balance_at(TEXT, BIGINT) SECURITY DEFINER;
REVOKE ALL ON FUNCTION voting_cl8y_balance_at(TEXT, BIGINT) FROM PUBLIC;

CREATE OR REPLACE FUNCTION voting_bsc_cl8y_balance_at(p_wallet TEXT, p_block BIGINT)
RETURNS NUMERIC
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    r voting_registrations%ROWTYPE;
    v NUMERIC(78, 0);
    addr TEXT;
BEGIN
    addr := lower(trim(p_wallet));
    SELECT * INTO r
    FROM voting_registrations
    WHERE chain = 'bsc' AND wallet_address = addr;
    IF NOT FOUND THEN
        RETURN 0;
    END IF;
    IF p_block < r.registered_at_height THEN
        RETURN 0;
    END IF;
    SELECT balance INTO v
    FROM cl8y_bsc_balances
    WHERE wallet_address = r.wallet_address AND bsc_block <= p_block
    ORDER BY bsc_block DESC
    LIMIT 1;
    IF FOUND THEN
        RETURN v;
    END IF;
    RETURN r.initial_balance;
END;
$$;

ALTER FUNCTION voting_bsc_cl8y_balance_at(TEXT, BIGINT) SECURITY DEFINER;
REVOKE ALL ON FUNCTION voting_bsc_cl8y_balance_at(TEXT, BIGINT) FROM PUBLIC;
