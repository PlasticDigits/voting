-- Forward-only metadata. Existing electorate cannot be certified retrospectively.
CREATE TABLE public.ledger_checkpoints (
    chain TEXT PRIMARY KEY CHECK(chain IN ('terra','bsc')),
    height BIGINT NOT NULL DEFAULT 0,
    block_hash TEXT NOT NULL DEFAULT '',
    observed_at TIMESTAMPTZ,
    epoch BIGINT NOT NULL DEFAULT 1,
    complete BOOLEAN NOT NULL DEFAULT false,
    canonical BOOLEAN NOT NULL DEFAULT false,
    degraded_reason TEXT,
    network_tip BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE public.ledger_canonical_blocks (
    chain TEXT NOT NULL CHECK(chain IN ('terra','bsc')),
    height BIGINT NOT NULL,
    block_hash TEXT NOT NULL,
    PRIMARY KEY(chain,height)
);
-- Preserve ambiguous legacy rows for audit, but make future source identities unique.
-- No transfer accounting may run until explicit live baseline reconciliation.
CREATE TABLE public.cl8y_cw20_legacy_identity_conflicts AS
SELECT t.* FROM public.cl8y_cw20_transfers t
WHERE (t.tx_hash,t.msg_index) IN (SELECT tx_hash,msg_index FROM public.cl8y_cw20_transfers GROUP BY tx_hash,msg_index HAVING COUNT(*)>1);
DELETE FROM public.cl8y_cw20_transfers t USING public.cl8y_cw20_legacy_identity_conflicts c WHERE t.id=c.id;
CREATE UNIQUE INDEX cl8y_cw20_source_identity ON public.cl8y_cw20_transfers(tx_hash,msg_index);
ALTER TABLE voting.registration_intents ADD COLUMN last_attempt_at TIMESTAMPTZ;
ALTER TABLE voting.registration_intents ADD COLUMN retry_after TIMESTAMPTZ;
ALTER TABLE voting.registration_intents ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE voting.registration_intents ADD COLUMN last_error_class TEXT;
CREATE INDEX registration_intents_pending_fair ON voting.registration_intents(COALESCE(last_attempt_at,created_at)) WHERE processed_at IS NULL;
-- Fail closed on upgrade: old running balances lack validated completeness/hash evidence.
INSERT INTO public.ledger_checkpoints(chain,degraded_reason)
SELECT DISTINCT chain,'legacy_electorate_requires_reconciliation' FROM public.voting_registrations WHERE status='active';
ALTER TABLE public.voting_registrations ADD COLUMN baseline_height BIGINT;
UPDATE public.voting_registrations SET baseline_height=registered_at_height;
ALTER TABLE public.voting_registrations ADD COLUMN baseline_block_hash TEXT;
ALTER TABLE public.voting_registrations ADD COLUMN baseline_epoch BIGINT NOT NULL DEFAULT 1;
ALTER TABLE public.voting_registrations ADD COLUMN baseline_verified BOOLEAN NOT NULL DEFAULT false;
