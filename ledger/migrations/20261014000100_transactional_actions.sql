-- Issues #40/#42: complete revisions and immutable, independently verifiable actions.
ALTER TABLE voting.proposals ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0);
ALTER TABLE voting.proposals ADD COLUMN snapshot_validity TEXT NOT NULL DEFAULT 'valid' CHECK (snapshot_validity IN ('valid','invalid','legacy_unverified'));
ALTER TABLE voting.proposals ADD COLUMN snapshot_incident TEXT;
UPDATE voting.proposals SET snapshot_validity='legacy_unverified',snapshot_incident='pre_checkpoint_freeze_cannot_be_verified' WHERE status='open';
ALTER TABLE voting.proposals ADD COLUMN terra_block_hash TEXT;
ALTER TABLE voting.proposals ADD COLUMN bsc_block_hash TEXT;
ALTER TABLE voting.proposals ADD COLUMN terra_epoch BIGINT;
ALTER TABLE voting.proposals ADD COLUMN bsc_epoch BIGINT;
ALTER TABLE voting.signatures ADD COLUMN signed_payload TEXT;
ALTER TABLE voting.signatures ADD COLUMN scheme TEXT;
ALTER TABLE voting.signatures ADD COLUMN domain TEXT;
ALTER TABLE voting.signatures ADD COLUMN nonce TEXT;
ALTER TABLE voting.signatures ADD COLUMN proposal_id UUID REFERENCES voting.proposals(id);
ALTER TABLE voting.signatures ADD COLUMN revision BIGINT;
CREATE UNIQUE INDEX signatures_action_nonce ON voting.signatures(chain,wallet_address,nonce) WHERE nonce IS NOT NULL;
ALTER TABLE voting.proposal_analysis ADD COLUMN revision BIGINT;
CREATE TABLE voting.action_receipts (
 signature_id UUID PRIMARY KEY REFERENCES voting.signatures(id),
 proposal_id UUID REFERENCES voting.proposals(id),
 revision BIGINT,
 response JSONB NOT NULL,
 content JSONB,
 accepted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE voting.proposal_revisions (
 proposal_id UUID NOT NULL REFERENCES voting.proposals(id),
 revision BIGINT NOT NULL,
 title TEXT NOT NULL,
 body_canonical TEXT NOT NULL,
 body_sections JSONB,
 signature_id UUID REFERENCES voting.signatures(id),
 recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(proposal_id,revision)
);
-- Legacy history is retained but has no verifiable original signed bytes.
INSERT INTO voting.proposal_revisions(proposal_id,revision,title,body_canonical,body_sections)
SELECT id,revision,title,body_canonical,body_sections FROM voting.proposals;
CREATE FUNCTION voting.reject_immutable_mutation() RETURNS trigger
 LANGUAGE plpgsql SET search_path = pg_catalog, public, voting, pg_temp AS $$
BEGIN RAISE EXCEPTION 'immutable voting record' USING ERRCODE='42501'; END $$;
CREATE TRIGGER immutable_signatures BEFORE UPDATE OR DELETE ON voting.signatures FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE TRIGGER immutable_action_receipts BEFORE UPDATE OR DELETE ON voting.action_receipts FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE TRIGGER immutable_proposal_revisions BEFORE UPDATE OR DELETE ON voting.proposal_revisions FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE TRIGGER immutable_votes BEFORE UPDATE OR DELETE ON voting.votes FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE TRIGGER immutable_proposal_snapshots BEFORE UPDATE OR DELETE ON voting.proposal_snapshots FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE TRIGGER immutable_proposal_comments BEFORE UPDATE OR DELETE ON voting.proposal_comments FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE TRIGGER immutable_proposal_analysis BEFORE UPDATE OR DELETE ON voting.proposal_analysis FOR EACH ROW EXECUTE FUNCTION voting.reject_immutable_mutation();
CREATE FUNCTION voting.guard_proposal_mutation() RETURNS trigger
 LANGUAGE plpgsql SET search_path = pg_catalog, public, voting, pg_temp AS $$
BEGIN
 IF OLD.status = 'open' AND (NEW.title,NEW.body_canonical,NEW.body_sections,NEW.terra_height,NEW.bsc_block,NEW.revision,NEW.opened_at,NEW.status)
 IS DISTINCT FROM (OLD.title,OLD.body_canonical,OLD.body_sections,OLD.terra_height,OLD.bsc_block,OLD.revision,OLD.opened_at,OLD.status)
 THEN RAISE EXCEPTION 'opened proposal is immutable' USING ERRCODE='42501'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER guard_proposals BEFORE UPDATE ON voting.proposals FOR EACH ROW EXECUTE FUNCTION voting.guard_proposal_mutation();
CREATE OR REPLACE FUNCTION public.voting_cl8y_balance_at(p_wallet TEXT, p_height BIGINT)
RETURNS NUMERIC
LANGUAGE plpgsql
STABLE
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
    r public.voting_registrations%ROWTYPE;
    v NUMERIC(78, 0);
BEGIN
    SELECT * INTO r
    FROM public.voting_registrations
    WHERE chain = 'terra' AND wallet_address = lower(trim(p_wallet));
    IF NOT FOUND THEN
        RETURN 0;
    END IF;
    IF p_height < r.registered_at_height THEN
        RETURN 0;
    END IF;
    SELECT balance INTO v
    FROM public.cl8y_balances
    WHERE wallet_address = r.wallet_address AND height <= p_height
    ORDER BY height DESC
    LIMIT 1;
    IF FOUND THEN
        RETURN v;
    END IF;
    RETURN r.initial_balance;
END;
$$;

ALTER FUNCTION public.voting_cl8y_balance_at(TEXT, BIGINT) SECURITY DEFINER;
REVOKE ALL ON FUNCTION public.voting_cl8y_balance_at(TEXT, BIGINT) FROM PUBLIC;

CREATE OR REPLACE FUNCTION public.voting_bsc_cl8y_balance_at(p_wallet TEXT, p_block BIGINT)
RETURNS NUMERIC
LANGUAGE plpgsql
STABLE
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
    r public.voting_registrations%ROWTYPE;
    v NUMERIC(78, 0);
    addr TEXT;
BEGIN
    addr := lower(trim(p_wallet));
    SELECT * INTO r
    FROM public.voting_registrations
    WHERE chain = 'bsc' AND wallet_address = addr;
    IF NOT FOUND THEN
        RETURN 0;
    END IF;
    IF p_block < r.registered_at_height THEN
        RETURN 0;
    END IF;
    SELECT balance INTO v
    FROM public.cl8y_bsc_balances
    WHERE wallet_address = r.wallet_address AND bsc_block <= p_block
    ORDER BY bsc_block DESC
    LIMIT 1;
    IF FOUND THEN
        RETURN v;
    END IF;
    RETURN r.initial_balance;
END;
$$;

ALTER FUNCTION public.voting_bsc_cl8y_balance_at(TEXT, BIGINT) SECURITY DEFINER;
REVOKE ALL ON FUNCTION public.voting_bsc_cl8y_balance_at(TEXT, BIGINT) FROM PUBLIC;
-- Freeze is the only API-role route to frozen snapshots and opening metadata.
CREATE FUNCTION voting.freeze_proposal(p_id UUID,p_signature_id UUID)
RETURNS TABLE(terra_height BIGINT,bsc_block BIGINT)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, voting, pg_temp AS $$
DECLARE t public.ledger_checkpoints%ROWTYPE; b public.ledger_checkpoints%ROWTYPE; p voting.proposals%ROWTYPE; s voting.signatures%ROWTYPE; payload JSONB;
BEGIN
 PERFORM pg_advisory_xact_lock(8136,1); PERFORM pg_advisory_xact_lock(8136,2);
 SELECT * INTO p FROM voting.proposals WHERE id=p_id FOR NO KEY UPDATE;
 SELECT * INTO s FROM voting.signatures WHERE id=p_signature_id;
 payload:=s.signed_payload::jsonb;
 IF p.status IS DISTINCT FROM 'draft' OR s.purpose IS DISTINCT FROM 'open_vote'
 OR s.proposal_id IS DISTINCT FROM p_id OR s.revision IS DISTINCT FROM p.revision
 OR (payload->>'revision')::bigint IS DISTINCT FROM p.revision
 OR payload->>'title' IS DISTINCT FROM p.title
 OR payload->>'body_hash' IS DISTINCT FROM encode(sha256(convert_to(p.body_canonical,'UTF8')),'hex')
 THEN RAISE EXCEPTION 'current complete signed revision required for opening'; END IF;
 SELECT * INTO t FROM public.ledger_checkpoints WHERE chain='terra';
 SELECT * INTO b FROM public.ledger_checkpoints WHERE chain='bsc';
 IF t.height IS NULL OR b.height IS NULL OR t.height<=0 OR b.height<=0
 OR t.observed_at IS NULL OR b.observed_at IS NULL OR t.block_hash='' OR b.block_hash=''
 OR t.network_tip<t.height OR b.network_tip<b.height
 OR t.network_tip-t.height > 2 OR b.network_tip-b.height > 17
 OR NOT t.complete OR NOT b.complete OR NOT t.canonical OR NOT b.canonical
 OR t.degraded_reason IS NOT NULL OR b.degraded_reason IS NOT NULL
 OR t.observed_at < now()-interval '120 seconds' OR b.observed_at < now()-interval '120 seconds'
 OR t.observed_at > now()+interval '60 seconds' OR b.observed_at > now()+interval '60 seconds'
 THEN RAISE EXCEPTION 'dual-chain ledger not ready for freeze'; END IF;
 UPDATE voting.proposals SET status='open',snapshot_validity='valid',terra_height=t.height,bsc_block=b.height,opened_at=now(),
 terra_block_hash=t.block_hash,bsc_block_hash=b.block_hash,terra_epoch=t.epoch,bsc_epoch=b.epoch
 WHERE id=p_id AND status='draft';
 IF NOT FOUND THEN RAISE EXCEPTION 'proposal is not a draft'; END IF;
 INSERT INTO voting.proposal_snapshots(proposal_id,chain,wallet_address,weight)
 SELECT p_id,r.chain,r.wallet_address,CASE r.chain WHEN 'terra' THEN public.voting_cl8y_balance_at(r.wallet_address,t.height)
 ELSE public.voting_bsc_cl8y_balance_at(r.wallet_address,b.height) END
 FROM public.voting_registrations r WHERE r.status='active';
 RETURN QUERY SELECT t.height,b.height;
END $$;
REVOKE ALL ON FUNCTION voting.freeze_proposal(UUID,UUID) FROM PUBLIC;

-- Database callers cannot forge eligibility/weight or append analysis after open.
CREATE FUNCTION voting.guard_vote_insert() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,public,voting,pg_temp AS $$
DECLARE p voting.proposals%ROWTYPE; w NUMERIC; s voting.signatures%ROWTYPE;
BEGIN
 SELECT * INTO p FROM voting.proposals WHERE id=NEW.proposal_id FOR NO KEY UPDATE;
 SELECT weight INTO w FROM voting.proposal_snapshots WHERE proposal_id=NEW.proposal_id AND chain=NEW.chain AND wallet_address=NEW.wallet_address;
 SELECT * INTO s FROM voting.signatures WHERE id=NEW.signature_id;
 IF p.status IS DISTINCT FROM 'open' OR p.snapshot_validity IS DISTINCT FROM 'valid' OR w IS NULL OR w<=0 OR NEW.weight IS DISTINCT FROM w
 OR s.purpose IS DISTINCT FROM 'vote' OR s.chain IS DISTINCT FROM NEW.chain OR s.wallet_address IS DISTINCT FROM NEW.wallet_address
 OR s.proposal_id IS DISTINCT FROM NEW.proposal_id OR s.signed_payload::jsonb->>'choice' IS DISTINCT FROM NEW.choice
 THEN RAISE EXCEPTION 'invalid frozen vote' USING ERRCODE='42501'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER guard_votes BEFORE INSERT ON voting.votes FOR EACH ROW EXECUTE FUNCTION voting.guard_vote_insert();
CREATE FUNCTION voting.guard_analysis_insert() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,public,voting,pg_temp AS $$
DECLARE p voting.proposals%ROWTYPE; n BIGINT;
BEGIN
 SELECT * INTO p FROM voting.proposals WHERE id=NEW.proposal_id FOR NO KEY UPDATE;
 SELECT count(*) INTO n FROM voting.proposal_analysis WHERE proposal_id=NEW.proposal_id AND chain=NEW.chain AND wallet_address=NEW.wallet_address;
 IF p.status IS DISTINCT FROM 'draft' OR p.proposer=NEW.wallet_address OR NEW.revision IS DISTINCT FROM p.revision OR n>=5
 THEN RAISE EXCEPTION 'invalid analysis state/revision/quota' USING ERRCODE='42501'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER guard_analysis BEFORE INSERT ON voting.proposal_analysis FOR EACH ROW EXECUTE FUNCTION voting.guard_analysis_insert();
