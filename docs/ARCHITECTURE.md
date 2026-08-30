# Architecture

```
  Terra LCD          BSC JSON-RPC
      │                   │
      ▼                   ▼
 ┌────────────────────────────────┐
 │  voting-ledger                 │  writer role
 │  register live balance         │
 │  CW20 wasm events (CL8Y only)  │
 │  BEP-20 Transfer logs (CL8Y)   │
 └────────────────┬───────────────┘
                  │ SECURITY DEFINER
                  │ voting_cl8y_balance_at
                  │ voting_bsc_cl8y_balance_at
                  ▼
 ┌────────────────────────────────┐
 │  operator-voting (Axum)        │  restricted role
 │  ADR-36 + EIP-191              │
 │  proposals / votes / blacklist │
 └────────────────┬───────────────┘
                  │ HTTPS
                  ▼
 ┌────────────────────────────────┐
 │  voting dApp                   │
 │  Legal TermsGate (required)    │
 │  Terra wallet ← DEX patterns   │
 │  EVM wallet   ← Bridge patterns│
 └────────────────────────────────┘
```

## Layout

```
ledger/              # Rust: ingest + Postgres migrations + balance functions
operator-voting/     # Rust Axum: signatures, proposals, votes
frontend/            # Vite React: /, /new, /:id (+ /vote* aliases, #8)
docs/ skills/        # invariants + agent playbooks
deploy/              # grants.sql, Coolify env, Dockerfiles (issue #7)
docs/OPS.md          # Coolify / Legal / live QA / rate limits
```

Three deployables (ledger worker, API, dApp) and **two** DB roles. Coolify images: [`../deploy/docker/`](../deploy/docker/). Public-expose checklist: [OPS.md](OPS.md) (issue [#7](https://gitlab.com/PlasticDigits/voting/-/issues/7)). SPA documents for Legal return: [FRONTEND.md](FRONTEND.md) (issue [#8](https://gitlab.com/PlasticDigits/voting/-/issues/8), invariant O8). Default GET `/v1/balances` clamp: [OPERATOR_VOTING.md](OPERATOR_VOTING.md) OV-B1 (issue [#9](https://gitlab.com/PlasticDigits/voting/-/issues/9)). WalletConnect pairing (cosmes patch, project id, CSP frames): [FRONTEND.md](FRONTEND.md) (issue [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)). WalletConnect pairing (Galaxy / iPhone / desktop Chrome): [FRONTEND.md](FRONTEND.md) WC-M1–M12 (issue [#12](https://gitlab.com/PlasticDigits/voting/-/issues/12)).

## Snapshot

A proposal freezes `{ terra_height, bsc_block }` at create time (wall-clock aligned). Terra voters use the CW20 ledger at that height. EVM voters use the BEP-20 ledger at that block. Do not convert via USD.

## Double-count

Today the CW20 and BEP-20 supplies are separately circulating (not lock-and-mint in this repo). Each address votes its own chain balance. If an official bridge later locks units, exclude the lock address explicitly.

## Related on-chain governance (not this product)

DEX factory `governance` / wasm admin is the multisig `terra1zlmv2xydxcusurtr6rl78wsvytdc6mfex6hep7` in `cl8y-dex-terraclassic`. Offchain polls here do **not** migrate contracts or change factory fees. UI copy must not imply on-chain DAO finality.
