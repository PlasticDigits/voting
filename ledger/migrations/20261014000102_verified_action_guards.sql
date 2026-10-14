-- Forward security audit followup: effective column privileges and real acceptance clocks.
ALTER TABLE voting.action_receipts ALTER COLUMN accepted_at SET DEFAULT clock_timestamp();
DO $$ DECLARE r RECORD; cols TEXT; BEGIN
 IF EXISTS(SELECT FROM pg_roles WHERE rolname='operator_voting') THEN
 FOR r IN SELECT c.oid,n.nspname,c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
 WHERE c.relkind='r' AND (n.nspname='voting' OR (n.nspname='public' AND c.relname IN ('voting_registrations','cl8y_balances','cl8y_bsc_balances','cl8y_cw20_transfers','cl8y_bep20_transfers','indexer_state','ledger_checkpoints','ledger_canonical_blocks')))
 LOOP
  SELECT string_agg(quote_ident(attname),',') INTO cols FROM pg_attribute WHERE attrelid=r.oid AND attnum>0 AND NOT attisdropped;
  IF cols IS NOT NULL THEN EXECUTE format('REVOKE ALL PRIVILEGES(%s) ON TABLE %I.%I FROM operator_voting,PUBLIC',cols,r.nspname,r.relname); END IF;
  EXECUTE format('REVOKE ALL ON TABLE %I.%I FROM operator_voting,PUBLIC',r.nspname,r.relname);
 END LOOP;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA voting FROM operator_voting;
ALTER DEFAULT PRIVILEGES IN SCHEMA voting REVOKE ALL ON TABLES FROM operator_voting;
ALTER DEFAULT PRIVILEGES IN SCHEMA voting REVOKE ALL ON SEQUENCES FROM operator_voting;
GRANT SELECT ON public.voting_registrations,public.indexer_state,public.ledger_checkpoints,public.ledger_canonical_blocks TO operator_voting;
GRANT SELECT ON voting.signatures,voting.registration_intents,voting.proposals,voting.proposal_snapshots,
 voting.votes,voting.proposal_comments,voting.proposal_analysis,voting.action_receipts,voting.proposal_revisions TO operator_voting;
GRANT INSERT ON voting.signatures,voting.votes,
 voting.proposal_comments,voting.proposal_analysis,voting.action_receipts,voting.proposal_revisions TO operator_voting;
GRANT INSERT(chain,wallet_address,signature_id) ON voting.registration_intents TO operator_voting;
GRANT INSERT(id,chain,proposer,title,body_html,body_canonical,body_sections) ON voting.proposals TO operator_voting;
GRANT UPDATE(title,body_html,body_canonical,body_sections,revision) ON voting.proposals TO operator_voting;
GRANT EXECUTE ON FUNCTION public.voting_cl8y_balance_at(TEXT,BIGINT),public.voting_bsc_cl8y_balance_at(TEXT,BIGINT),voting.freeze_proposal(UUID,UUID) TO operator_voting;
REVOKE ALL ON FUNCTION public.voting_cl8y_balance_at(TEXT,BIGINT),public.voting_bsc_cl8y_balance_at(TEXT,BIGINT),voting.freeze_proposal(UUID,UUID) FROM PUBLIC;
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON public.voting_registrations,public.cl8y_balances,public.cl8y_bsc_balances,
 public.cl8y_cw20_transfers,public.cl8y_bep20_transfers,public.indexer_state,public.ledger_checkpoints,public.ledger_canonical_blocks FROM operator_voting;
-- No default grants: each future table requires an explicit reviewed matrix change.

 END IF;
END $$;
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
