-- Opening requires a fresh successful bounded worker pass, never pending-item age.
CREATE OR REPLACE FUNCTION voting.freeze_proposal(p_id UUID,p_signature_id UUID)
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
 OR t.observed_at < clock_timestamp()-interval '120 seconds' OR b.observed_at < clock_timestamp()-interval '120 seconds'
 OR t.observed_at > clock_timestamp()+interval '60 seconds' OR b.observed_at > clock_timestamp()+interval '60 seconds'
 THEN RAISE EXCEPTION 'dual-chain ledger not ready for freeze'; END IF;
 IF EXISTS(SELECT 1 FROM public.voting_registrations r WHERE r.status='active'
 AND COALESCE(r.baseline_height,r.registered_at_height)<=CASE r.chain WHEN 'terra' THEN t.height ELSE b.height END
 AND (NOT r.baseline_verified OR r.baseline_block_hash IS NULL OR r.baseline_block_hash=''))
 THEN RAISE EXCEPTION 'eligible registration baseline not verified'; END IF;
 IF NOT EXISTS(SELECT 1 FROM public.indexer_state WHERE key='intents_ok' AND value='true' AND updated_at BETWEEN clock_timestamp()-interval '120 seconds' AND clock_timestamp()) THEN RAISE EXCEPTION 'intent worker pass heartbeat unavailable'; END IF;
 UPDATE voting.proposals SET status='open',snapshot_validity='valid',terra_height=t.height,bsc_block=b.height,opened_at=clock_timestamp(),
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
