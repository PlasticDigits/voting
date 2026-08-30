-- Structured proposal sections (issue #10).
-- JSONB is the source of truth for new rows. Legacy rows keep body_html only.
-- Operator-voting already has GRANT ALL on voting.proposals; no grants.sql change.

ALTER TABLE voting.proposals
    ADD COLUMN IF NOT EXISTS body_sections JSONB;

ALTER TABLE voting.proposals
    DROP CONSTRAINT IF EXISTS proposals_body_sections_shape;

ALTER TABLE voting.proposals
    ADD CONSTRAINT proposals_body_sections_shape CHECK (
        body_sections IS NULL
        OR (
            jsonb_typeof(body_sections) = 'object'
            AND jsonb_typeof(body_sections -> 'problem') = 'string'
            AND jsonb_typeof(body_sections -> 'context') = 'string'
            AND jsonb_typeof(body_sections -> 'solution') = 'string'
            AND jsonb_typeof(body_sections -> 'pros_cons') = 'string'
            AND jsonb_typeof(body_sections -> 'summary') = 'string'
            AND jsonb_typeof(body_sections -> 'success_criteria') = 'string'
        )
    );

COMMENT ON COLUMN voting.proposals.body_sections IS
    'Canonical section object (issue #10). Null = legacy freeform body_html.';
