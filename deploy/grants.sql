-- Issues #40/#42. Apply as migration owner with psql ON_ERROR_STOP=1.
-- No usable default password: provision login credentials through bao-exec.
DO $$ BEGIN
 IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='operator_voting') THEN
  CREATE ROLE operator_voting LOGIN;
 END IF;
END $$;
DO $$ BEGIN
 EXECUTE format('GRANT CONNECT ON DATABASE %I TO operator_voting',current_database());
 EXECUTE format('REVOKE CREATE,TEMPORARY ON DATABASE %I FROM operator_voting',current_database());
 EXECUTE format('REVOKE CREATE,TEMPORARY ON DATABASE %I FROM PUBLIC',current_database());
END $$;
REVOKE CREATE ON SCHEMA public,voting FROM PUBLIC,operator_voting;
GRANT USAGE ON SCHEMA public,voting TO operator_voting;
REVOKE ALL ON ALL TABLES IN SCHEMA voting FROM operator_voting;
-- Table revocation does not remove column-only grants. Clear both surfaces.
DO $$ DECLARE r RECORD; cols TEXT; BEGIN
 FOR r IN SELECT c.oid,n.nspname,c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
 WHERE c.relkind='r' AND (n.nspname='voting' OR (n.nspname='public' AND c.relname IN ('voting_registrations','cl8y_balances','cl8y_bsc_balances','cl8y_cw20_transfers','cl8y_bep20_transfers','indexer_state','ledger_checkpoints','ledger_canonical_blocks')))
 LOOP
  SELECT string_agg(quote_ident(attname),',') INTO cols FROM pg_attribute WHERE attrelid=r.oid AND attnum>0 AND NOT attisdropped;
  IF cols IS NOT NULL THEN EXECUTE format('REVOKE ALL PRIVILEGES(%s) ON TABLE %I.%I FROM operator_voting,PUBLIC',cols,r.nspname,r.relname); END IF;
  EXECUTE format('REVOKE ALL ON TABLE %I.%I FROM operator_voting,PUBLIC',r.nspname,r.relname);
 END LOOP;
END $$;

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
