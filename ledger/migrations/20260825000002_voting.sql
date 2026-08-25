-- Control-plane tables. Ledger writer applies these; operator-voting uses a restricted role.
-- See deploy/grants.sql.

CREATE SCHEMA IF NOT EXISTS voting;

CREATE TABLE IF NOT EXISTS voting.signatures (
    id UUID PRIMARY KEY,
    chain TEXT NOT NULL CHECK (chain IN ('terra', 'bsc')),
    wallet_address TEXT NOT NULL,
    pubkey TEXT,
    signature TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    purpose TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS signatures_wallet_idx
    ON voting.signatures (chain, wallet_address, purpose);

CREATE TABLE IF NOT EXISTS voting.registration_intents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chain TEXT NOT NULL CHECK (chain IN ('terra', 'bsc')),
    wallet_address TEXT NOT NULL,
    signature_id UUID NOT NULL REFERENCES voting.signatures(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ,
    UNIQUE (chain, wallet_address)
);

CREATE TABLE IF NOT EXISTS voting.proposals (
    id UUID PRIMARY KEY,
    chain TEXT NOT NULL CHECK (chain IN ('terra', 'bsc')),
    proposer TEXT NOT NULL,
    title TEXT NOT NULL,
    body_html TEXT NOT NULL,
    body_canonical TEXT NOT NULL,
    terra_height BIGINT NOT NULL,
    bsc_block BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL DEFAULT 'open'
);

CREATE TABLE IF NOT EXISTS voting.proposal_snapshots (
    proposal_id UUID NOT NULL REFERENCES voting.proposals(id),
    chain TEXT NOT NULL,
    wallet_address TEXT NOT NULL,
    weight NUMERIC(78, 0) NOT NULL,
    PRIMARY KEY (proposal_id, chain, wallet_address)
);

CREATE TABLE IF NOT EXISTS voting.votes (
    proposal_id UUID NOT NULL REFERENCES voting.proposals(id),
    wallet_address TEXT NOT NULL,
    chain TEXT NOT NULL,
    choice TEXT NOT NULL CHECK (choice IN ('for', 'against', 'abstain')),
    weight NUMERIC(78, 0) NOT NULL,
    signature_id UUID NOT NULL REFERENCES voting.signatures(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (proposal_id, chain, wallet_address)
);
