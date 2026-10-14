# Transactional voting actions and receipt verification

Read [handoff](../docs/HANDOFF.md), [governance doctrine](../docs/GOVERNANCE_RESEARCH.md),
and [OV-T1–OV-T6](../docs/OPERATOR_VOTING.md#transactional-signed-actions-issues-40-and-42)
before changing signed writes. Issues: [#40](https://git.cl8y.com/code/voting/issues/40),
[#42](https://git.cl8y.com/code/voting/issues/42). Keep the [draft review skill](AGENTS_DRAFT_REVIEW.md)
and [Legal skill](AGENTS_LEGAL_CLICKWRAP.md) in the workflow.

1. Get `/v1/metadata` and the current proposal before constructing a v2 payload.
   Keep the original signed JSON bytes. Generate a fresh nonce for a new action;
   retry the **same bytes** on uncertain responses. Never re-sign with a consumed
   nonce and different content. Exact configured network, app, purpose, address
   and chain are signing domains; Terra and BSC identities remain separate.
2. Bind amendment/analysis/opening to current `revision`. Amendments also bind
   title, canonical section hash and previous section hash. Opening binds title
   and canonical section hash as well as revision; title-only edits invalidate
   earlier approvals. Use server canonicalization before asking a wallet to
   sign. Do not sign opening client heights: validated writer checkpoints own
   freeze height/hash/epoch and materialized electorate power.
3. Keep all state validation, quotas, signature persistence, mutation and receipt
   acceptance within one transaction. Acquire chain locks in Terra/BSC order
   before opening/voting and proposal `FOR NO KEY UPDATE` before lifecycle checks.
   Never move revision or quota checks outside that transaction. Preserve
   immutable revision histories and reject invalidated/legacy-unverified freezes.
4. Verify receipt exports with `operator_voting::payload::verify_exported_receipt`
   (offline, without fresh time checks). Verify signature/content tamper failures
   too. Disclose that wallet receipts prove signed intent; checkpoint/frozen
   power is operator ledger evidence. Legacy hash-only rows are not reconstructable.
5. Apply only forward migrations through the writer. API role must pass
   `db::validate_production_privileges`; use [granular grants](../deploy/grants.sql),
   no default passwords/blanket grants. Never use the writer URL for API startup.
6. For destructive tests use an ephemeral `voting_test_*` database on loopback,
   explicit `LEDGER_TEST_DATABASE_URL`, `LEDGER_TEST_ALLOW_DESTRUCTIVE=1`, and
   `LEDGER_TEST_REQUIRED=1`. The guard must run before connecting or running DDL.
   Never use application `DATABASE_URL` as a test fallback. Keep credential values
   out of logs and use the authorized `bao-exec` workflow for production secrets.

Verification: guarded Rust integration suites include both ADR-36/EIP-191
lifecycles, simultaneous stale amendments, duplicate action acceptance,
parallel analysis quotas, receipt tampering, original temporary-table 999
balance attack, populated upgrade preservation and actual restricted-role
permission checks. Run unit tests and release timestamp regressions as well.
Report which additional browser/live/process-restart scenarios were actually
executed; do not infer production completeness from isolated integration tests.

The runtime test database catalog comment must equal `cl8y-voting-ephemeral-test`.
Provision this marker only on a new disposable database; an absent/wrong marker
fails before migration or destructive SQL. Never mark an application database.

Follow [OV-T7–OV-T10](../docs/OPERATOR_VOTING.md#review-followup-shared-quotas-actual-acceptance-and-field-level-evidence)
when modifying quotas, clocks or operational privileges. Shared wallet quotas
must use authenticated actors and accepted database receipts under the wallet
lock; unsigned envelopes cannot charge another actor. Check signed TTL again at
actual acceptance and roll back expired mutations. Preserve exact content and
revision links in offline verification. Audit column grants as well as table
permissions, and leave ledger queue processing metadata to the writer. Eligible
unverified baselines block opening; above-cutoff registrations do not. Policy
rotation requires an API restart and unaccepted old-role signatures must fail.

Keep canonical v2 optional revision after expires_at; use the shared wire vectors
for Unicode and ADR-36/EIP-191 signing bytes. Native Terra signatures must use the
terra prefix. The database API identity must authenticate directly, have no parent
role memberships, and own no protected definers. Worker readiness requires a fresh
successful intents_ok pass, not an empty queue or arbitrary per-intent age cutoff.
