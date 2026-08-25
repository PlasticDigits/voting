# operator-voting

Offchain voting control plane. Threat model and API: [`../docs/OPERATOR_VOTING.md`](../docs/OPERATOR_VOTING.md).

Production `DATABASE_URL` must be the restricted role from [`../deploy/grants.sql`](../deploy/grants.sql) — never the ledger writer.

```bash
DATABASE_URL=postgres://operator_voting:… cargo run -p operator-voting
# OpenAPI stub: GET /openapi.json
```

Coolify env: [`../deploy/coolify.env.example`](../deploy/coolify.env.example).
