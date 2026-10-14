---
name: voting-protocol-security
description: >-
  Voting API middleware, browser HTML rendering, canonical signing bytes, generated
  API contracts and production mode boundaries (issues 41 and 43).
---

# Protocol and rendering security

Read [FRONTEND.md](../docs/FRONTEND.md), [OPERATOR_VOTING.md](../docs/OPERATOR_VOTING.md), [OPS.md](../docs/OPS.md) and [GOVERNANCE_RESEARCH.md](../docs/GOVERNANCE_RESEARCH.md) before changing this boundary. Issues: [41](https://git.cl8y.com/code/voting/issues/41), [43](https://git.cl8y.com/code/voting/issues/43).

## Invariants

- **P1:** `application::application` owns the production HTTP stack. Tests must exercise it; `api::router` alone excludes middleware. Origin policy applies exact validated HTTP(S) origins, GET/POST/PUT/OPTIONS and Content-Type/Accept headers. No wildcard/reflection. Missing Origin supports direct agent clients; null and unknown Origins fail closed.
- **P2:** every POST/PUT/PATCH/DELETE shares an IP quota; signed mutations also enforce a verified action/wallet quota after domain/signature/time validation and nonce receipt lookup. Unauthenticated envelopes never debit wallet buckets; accepted receipt retries bypass wallet charging. Normalize wallet keys exactly as signature authentication does. IP token buckets expire after ten minutes of inactivity and have a 10,000-entry cap plus an overflow bucket. Saturating the cap cannot reset live buckets. 429 exposes Retry-After. Canonicalization POSTs are also IP bounded. Verified wallet/action enforcement counts accepted receipts under a PostgreSQL advisory lock so replicas share that quota; safe receipt retries return before charging.
- **P3:** forwarding is off by default. `RATE_LIMIT_TRUST_FORWARDED=true` requires `RATE_LIMIT_TRUSTED_PROXY_CIDRS`. Walk XFF from the authenticated peer right to left to the first untrusted hop. Reject malformed, duplicate, oversized and >16-hop headers by falling back to peer identity. X-Real-IP is never trusted. Ingress must prevent direct public access and the trusted edge must replace incoming client forwarding headers. Record topology evidence before claiming a live pass.
- **P4:** IP quotas are process-local; authenticated wallet/action quotas use the shared database. Production requires `VOTING_API_REPLICAS=1`; horizontal deployment requires a shared aggregate limiter before changing this rule. Setting a variable is not proof the platform runs one replica: inspect deployment topology.
- **P5:** all untrusted HTML reaches `SafeHtml`; DOMPurify permits only the documented WYSIWYG subset. No DOM means empty HTML. Never use raw `dangerouslySetInnerHTML` in another component. This includes legacy bodies, previews, sections, comments and analyses.
- **P6:** display sanitation never supplies signed bytes. POST `/v1/canonicalize` invokes authoritative ammonia and section validation. The client recomputes SHA256 of returned canonical JSON (or comment HTML), checks protocol version 2 and signs that hash, then submits the returned content. Changing the renderer must not rewrite signed bytes or immutable receipts.
- **P7:** lengths are UTF8 bytes via TextEncoder, not JS string length. Visible limits count Unicode scalars; invalid numeric entities remain text without exceptions. Shared fixtures: [canonical-v2.json](../protocol/fixtures/canonical-v2.json). Keep original v1 golden messages verifiable as history; new mutations require version 2 and a fresh nonce. Amendment/analysis/open bind the current revision; open also binds title and body hash. A stale conflict requires reloading and a new signature.
- **P8:** public errors expose stable code, generic message and request_id. Log request ID/status without query strings, database text, provider URLs or request bodies. Never add debugging that formats credentials or raw DB/provider errors.
- **P9:** prod/production are the same mode; dev/development are the same mode; unknown runtime spellings fail. Production networks are columbus-5 and BSC 56. Every Vite build, including custom modes, applies HTTPS API URL/network/simulation/browser RPC guards. Dev server fixtures are deliberately separate from production builds.
- **P10:** handler annotations plus typed response DTOs generate OpenAPI; TypeScript is generated from that artifact. Do not add a handwritten OpenAPI stub or duplicate API shapes. Amounts, including receipt snapshot weights, remain decimal strings. New routes must update generated artifacts and route contract tests. `snapshot_eligible` is future admission only: poll voting reads the exact stored `/v1/proposals/:id/weights/:addr` weight before enabling controls and immediately before signing. Invalid/legacy snapshots and zero/missing per-poll weight disable new votes; historical values stay visible. Never recompute frozen power from current/reconciled ledger balances.

## Verification

```bash
CARGO_BUILD_JOBS=2 cargo test --workspace --lib
CARGO_BUILD_JOBS=2 cargo test -p operator-voting --test application --test protocol_vectors
CARGO_BUILD_JOBS=2 cargo run -p operator-voting --example export_openapi > protocol/openapi.json
cd frontend
npm exec patch-package
npm test
VITE_OPERATOR_VOTING_URL=https://operator.vote.cl8y.com VITE_WC_PROJECT_ID=build-check npm run build
npm run generate:contracts
# Isolated PostgreSQL only; never point this browser fixture at production.
LEDGER_TEST_DATABASE_URL=postgres://voting:voting@127.0.0.1:55438/voting_test_protocol LEDGER_TEST_ALLOW_DESTRUCTIVE=1 npm exec playwright test -- --config protocol.playwright.config.ts
```

The browser fixture starts the real application middleware and uses an explicitly public, unfunded test wallet. It changes only a local `voting_test_` database. Live wallet signing, proxy topology and provider log checks need distinct dated evidence; local tests cannot stand in for them.

## One vote and signing wire compatibility

A poll keeps one immutable vote per address/chain. Load `/v1/proposals/:id/votes/:addr?chain=…` before offering signing controls and again immediately before signing. An accepted vote remains visible after reload and is scoped to its wallet, never a connected replacement account. Response-loss retries reuse exactly the original signed bytes and nonce; they do not request another wallet signature. Fresh-nonce duplicates conflict without appending signatures or votes.

[payload-v2.json](../protocol/fixtures/payload-v2.json) is the shared wire fixture, separate from HTML canonical fixtures. Regenerate via `cargo run -p operator-voting --example export_payload_vectors`. Both Rust and TypeScript verify exact serialized bytes, EIP-191 digest and ADR-36 sign-document/SHA256 for Terra/BSC register/amend/open, including Unicode titles, revision and optional-field omission. Do not reorder keys or rewrite persisted raw signed bytes. Title limits are UTF8 bytes; visible summary limits are Unicode scalars.
