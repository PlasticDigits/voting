# Issue migration

Source project: [PlasticDigits/cl8y-dex-terraclassic](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic) (id `80162261`)  
Destination: [PlasticDigits/voting](https://gitlab.com/PlasticDigits/voting)

GitLab **move** keeps comments and redirects the old URLs.

| DEX IID | Title (short) | Voting IID | Notes |
|---------|---------------|------------|-------|
| [#509](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/509) | Indexer registration + CL8Y CW20 ledger | _pending push_ | Data plane; also BSC after #588 |
| [#510](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/510) | `operator-voting` package | _pending push_ | Control plane; ADR-36 + EIP-191 |
| [#511](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/511) | Frontend `/vote` UX | _pending push_ | Legal gate + dual wallets required |
| [#588](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/588) | BSC BEP-20 + EVM voting | _pending push_ | Core electorate, not phase-2 |

New issues created in this repo after the move (not on DEX):

| Voting IID | Title |
|------------|-------|
| _pending_ | Legal clickwrap gate via cl8y-ecosystem-legal |
| _pending_ | Reuse DEX Terra Classic + Bridge EVM wallet connectors |

This table is filled in after `glab` move completes. See the bootstrap commit that updates this file.
