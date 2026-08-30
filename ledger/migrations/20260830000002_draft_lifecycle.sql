-- Draft collaboration lifecycle (issue #11).
-- Applies after 20260830000001_proposal_sections.sql.
-- Snapshot freeze moves to vote-open. New rows start as draft.
-- Existing open rows keep their freeze heights and stay votable.

ALTER TABLE voting.proposals
    ALTER COLUMN terra_height DROP NOT NULL,
    ALTER COLUMN bsc_block DROP NOT NULL,
    ALTER COLUMN status SET DEFAULT 'draft';

ALTER TABLE voting.proposals
    ADD COLUMN IF NOT EXISTS opened_at TIMESTAMPTZ;

UPDATE voting.proposals
SET opened_at = created_at
WHERE status = 'open' AND opened_at IS NULL;

ALTER TABLE voting.proposals
    DROP CONSTRAINT IF EXISTS proposals_status_check;
ALTER TABLE voting.proposals
    ADD CONSTRAINT proposals_status_check CHECK (status IN ('draft', 'open'));

ALTER TABLE voting.proposals
    DROP CONSTRAINT IF EXISTS proposals_open_freeze_check;
ALTER TABLE voting.proposals
    ADD CONSTRAINT proposals_open_freeze_check CHECK (
        status <> 'open'
        OR (terra_height IS NOT NULL AND bsc_block IS NOT NULL AND opened_at IS NOT NULL)
    );

CREATE TABLE IF NOT EXISTS voting.proposal_comments (
    id UUID PRIMARY KEY,
    proposal_id UUID NOT NULL REFERENCES voting.proposals(id),
    chain TEXT NOT NULL CHECK (chain IN ('terra', 'bsc')),
    wallet_address TEXT NOT NULL,
    body_html TEXT NOT NULL,
    signature_id UUID NOT NULL REFERENCES voting.signatures(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS proposal_comments_proposal_idx
    ON voting.proposal_comments (proposal_id, created_at);

CREATE TABLE IF NOT EXISTS voting.proposal_analysis (
    id UUID PRIMARY KEY,
    proposal_id UUID NOT NULL REFERENCES voting.proposals(id),
    chain TEXT NOT NULL CHECK (chain IN ('terra', 'bsc')),
    wallet_address TEXT NOT NULL,
    sections JSONB NOT NULL,
    body_html TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('committee', 'ai')),
    signature_id UUID NOT NULL REFERENCES voting.signatures(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS proposal_analysis_proposal_idx
    ON voting.proposal_analysis (proposal_id, created_at);
