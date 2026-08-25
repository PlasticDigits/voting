-- Run as the ledger owner after both migration sets.
-- Production: create operator_voting with a secret password (not this file).
-- This role must NOT write ledger ingest tables.

DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'operator_voting') THEN
    CREATE ROLE operator_voting LOGIN PASSWORD 'change-me-in-prod';
  END IF;
END
$$;

GRANT CONNECT ON DATABASE voting TO operator_voting;
GRANT USAGE ON SCHEMA public TO operator_voting;
GRANT USAGE ON SCHEMA voting TO operator_voting;

GRANT SELECT ON voting_registrations, indexer_state TO operator_voting;
GRANT EXECUTE ON FUNCTION voting_cl8y_balance_at(TEXT, BIGINT) TO operator_voting;
GRANT EXECUTE ON FUNCTION voting_bsc_cl8y_balance_at(TEXT, BIGINT) TO operator_voting;

GRANT ALL ON SCHEMA voting TO operator_voting;
GRANT ALL ON ALL TABLES IN SCHEMA voting TO operator_voting;
GRANT ALL ON ALL SEQUENCES IN SCHEMA voting TO operator_voting;
ALTER DEFAULT PRIVILEGES IN SCHEMA voting GRANT ALL ON TABLES TO operator_voting;

REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON
    voting_registrations, cl8y_balances, cl8y_bsc_balances,
    cl8y_cw20_transfers, cl8y_bep20_transfers, indexer_state
    FROM operator_voting;
