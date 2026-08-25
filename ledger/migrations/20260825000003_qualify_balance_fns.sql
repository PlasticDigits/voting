-- The writer role is often named `voting`, and migration 2 creates schema `voting`.
-- Postgres search_path is `"$user", public`, so an unqualified CREATE OR REPLACE
-- FUNCTION after that schema exists lands in schema `voting` and shadows the
-- SECURITY DEFINER originals in public (L10). Drop any copies. Always GRANT /
-- CALL public.voting_*_balance_at. See docs/LEDGER_INVARIANTS.md L10 and deploy/grants.sql.

DROP FUNCTION IF EXISTS voting.voting_cl8y_balance_at(TEXT, BIGINT);
DROP FUNCTION IF EXISTS voting.voting_bsc_cl8y_balance_at(TEXT, BIGINT);
