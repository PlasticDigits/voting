-- Upgrade an existing API role atomically with the writer migration.
-- Fresh installs provision a passwordless role using deploy/grants.sql.
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='operator_voting') THEN
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO operator_voting',current_database());
  EXECUTE format('REVOKE CREATE,TEMPORARY ON DATABASE %I FROM operator_voting',current_database());
  EXECUTE format('REVOKE CREATE,TEMPORARY ON DATABASE %I FROM PUBLIC',current_database());
REVOKE CREATE ON SCHEMA public,voting FROM PUBLIC,operator_voting;
GRANT USAGE ON SCHEMA public,voting TO operator_voting;
REVOKE ALL ON ALL TABLES IN SCHEMA voting FROM operator_voting;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA voting FROM operator_voting;
ALTER DEFAULT PRIVILEGES IN SCHEMA voting REVOKE ALL ON TABLES FROM operator_voting;
ALTER DEFAULT PRIVILEGES IN SCHEMA voting REVOKE ALL ON SEQUENCES FROM operator_voting;
GRANT SELECT ON public.voting_registrations,public.indexer_state,public.ledger_checkpoints,public.ledger_canonical_blocks TO operator_voting;
GRANT SELECT ON voting.signatures,voting.registration_intents,voting.proposals,voting.proposal_snapshots,
 voting.votes,voting.proposal_comments,voting.proposal_analysis,voting.action_receipts,voting.proposal_revisions TO operator_voting;
GRANT INSERT ON voting.signatures,voting.registration_intents,voting.votes,
 voting.proposal_comments,voting.proposal_analysis,voting.action_receipts,voting.proposal_revisions TO operator_voting;
GRANT INSERT(id,chain,proposer,title,body_html,body_canonical,body_sections) ON voting.proposals TO operator_voting;
GRANT UPDATE(title,body_html,body_canonical,body_sections,revision) ON voting.proposals TO operator_voting;
GRANT EXECUTE ON FUNCTION public.voting_cl8y_balance_at(TEXT,BIGINT),public.voting_bsc_cl8y_balance_at(TEXT,BIGINT),voting.freeze_proposal(UUID,UUID) TO operator_voting;
REVOKE ALL ON FUNCTION public.voting_cl8y_balance_at(TEXT,BIGINT),public.voting_bsc_cl8y_balance_at(TEXT,BIGINT),voting.freeze_proposal(UUID,UUID) FROM PUBLIC;
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON public.voting_registrations,public.cl8y_balances,public.cl8y_bsc_balances,
 public.cl8y_cw20_transfers,public.cl8y_bep20_transfers,public.indexer_state,public.ledger_checkpoints,public.ledger_canonical_blocks FROM operator_voting;
-- No default grants: each future table requires an explicit reviewed matrix change.

 END IF;
END $$;
