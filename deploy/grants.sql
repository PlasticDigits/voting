-- Run as the ledger owner after voting-ledger has applied sqlx migrations.
-- Production: create operator_voting with a secret password (not this file).
-- This role must NOT write ledger ingest tables.
-- Coolify order: ledger writer boot → this file → operator-voting (restricted URL).
-- Apply with: psql -v ON_ERROR_STOP=1 -d "$WRITER_URL" -f deploy/grants.sql
-- See docs/OPS.md (O1–O2) and docs/LEDGER_INVARIANTS.md (L10).
--
-- Invariant: GRANT EXECUTE must name public.voting_*_balance_at. The writer
-- role is often `voting` and schema `voting` exists, so unqualified names
-- follow search_path "$user", public and miss the SECURITY DEFINER originals.

DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'operator_voting') THEN
    CREATE ROLE operator_voting LOGIN PASSWORD 'change-me-in-prod';
  END IF;
END
$$;

-- Database name is not hardcoded so CI / local compose can share this file.
DO $$
BEGIN
  EXECUTE format('GRANT CONNECT ON DATABASE %I TO operator_voting', current_database());
END
$$;

GRANT USAGE ON SCHEMA public TO operator_voting;
GRANT USAGE ON SCHEMA voting TO operator_voting;

GRANT SELECT ON voting_registrations, indexer_state TO operator_voting;
-- Qualify public.* — the writer role is often named `voting`, so search_path
-- `"$user", public` would otherwise GRANT a copy in schema `voting` (L10).
GRANT EXECUTE ON FUNCTION public.voting_cl8y_balance_at(TEXT, BIGINT) TO operator_voting;
GRANT EXECUTE ON FUNCTION public.voting_bsc_cl8y_balance_at(TEXT, BIGINT) TO operator_voting;

GRANT ALL ON ALL TABLES IN SCHEMA voting TO operator_voting;
GRANT ALL ON ALL SEQUENCES IN SCHEMA voting TO operator_voting;
ALTER DEFAULT PRIVILEGES IN SCHEMA voting GRANT ALL ON TABLES TO operator_voting;
-- New voting.* tables (comments, analysis, …) are covered by ALL + default privileges.
-- Restricted role still cannot write ledger ingest (below).

REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON
    voting_registrations, cl8y_balances, cl8y_bsc_balances,
    cl8y_cw20_transfers, cl8y_bep20_transfers, indexer_state
    FROM operator_voting;
