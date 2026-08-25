# operator-voting

Offchain voting control plane. Threat model and API: [`../docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md).

Production `DATABASE_URL` must be the restricted role from [`../deploy/grants.sql`](../deploy/grants.sql) — never the ledger writer. `RUN_MODE=prod` skips sqlx migrations (`APPLY_MIGRATIONS=false`, refused if true), requires CORS plus a non-zero POST rate limit, and returns `Retry-After` on 429. Invariants: [`../docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md) O-RL · [`../docs/OPS.md`](../docs/OPS.md).

```bash
DATABASE_URL=postgres://operator_voting:… cargo run -p operator-voting
# OpenAPI stub: GET /openapi.json
```

Coolify env: [`../deploy/coolify.env.example`](../deploy/coolify.env.example).
