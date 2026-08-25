---
name: voting-wallet-connectors
description: >-
  Port Terra Classic wallet connect from CL8Y DEX and EVM wallet connect from
  CL8Y Bridge into the voting dApp. Use when adding connect, signArbitrary,
  personal_sign, WalletConnect, Keplr, MetaMask, or any voting wallet UX.
---

# Wallet connectors (voting)

For wallet connecting software, as there has been many problems with it, the terraclassic connections on cl8y dex and the evm connections on cl8y bridge are the most well functional.

**Do not** start from a greenfield wallet kit, a new WC pairing design, or a “simplified” connect modal. Copy the working stacks, then delete bridge/DEX features voting does not need (swaps, deposits, token lists).

## Terra Classic — copy from DEX

Repo: [cl8y-dex-terraclassic](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic)  
Primary files:

| Path | Why |
|------|-----|
| `frontend-dapp/src/services/terraclassic/wallet.ts` | Keplr / Station / WC / Simulated Wallet connect |
| `frontend-dapp/src/hooks/useWallet.ts` | Session store |
| `frontend-dapp/src/components/wallet/` | Modal, WC pairing UI |
| `frontend-dapp/src/services/terraclassic/walletConnectPairingHook.ts` | Mobile WC deep-link + copy (not QR-only) |
| `frontend-dapp/src/utils/walletConnectPairing.ts` | Pairing URI guards |
| `skills/AGENTS_FRONTEND_WALLETCONNECT_MOBILE.md` | WC-M1–WC-M12 |
| `skills/AGENTS_FRONTEND_KEPLR_LEDGER.md` | Amino / Ledger stalls |

Voting needs **`signArbitrary` (ADR-36)** for register / propose / vote. DEX today is tx-sign heavy; add arbitrary sign on top of the DEX connect path. Simulated Wallet must work for LocalTerra QA.

Do **not** use Terra Station as the primary LocalTerra QA wallet (DEX #235). Prefer Keplr or Simulated Wallet.

Do **not** break Cosmos WalletConnect mobile pairing when adding EIP-155.

## EVM — copy from Bridge

Repo: [cl8y-bridge-monorepo](https://gitlab.com/PlasticDigits/cl8y-bridge-monorepo)  
Primary files:

| Path | Why |
|------|-----|
| `packages/frontend/src/lib/wagmi.ts` | wagmi config, WC, Coinbase, simulated/mock in dev |
| `packages/frontend/src/hooks/useWallet.ts` | Connect/disconnect surface |
| `packages/frontend/src/hooks/useEvmWalletDiscovery.ts` | Injected EIP-1193 discovery |
| `packages/frontend/src/stores/wallet.ts` | Persistence |
| `packages/frontend/src/services/wallet.ts` | Re-exports (Terra side of bridge — do not prefer over DEX for Terra) |

Voting on EVM is **BSC (`eip155:56`)** for CL8Y BEP-20. Bridge also speaks opBNB / MegaETH / Anvil — keep those only as test fixtures, not as extra electorates, unless product expands later.

Use **EIP-191 `personal_sign`** for register / propose / vote. Injected/WC provider is for connect + sign only.

**No `VITE_*` BSC JSON-RPC URLs** for balance reads (same hygiene as DEX #571 V571-5). Ledger service owns `eth_call` / `eth_getLogs`.

## Legal portal is not a wallet

`https://terms.cl8y.com/sign/evm` and Terra Legal sign URLs are **terms acceptance**. They are not voting signatures. After Legal returns, the voting dApp still does its own ADR-36 / EIP-191 payloads.

## Support matrix (v1)

| Chain | Preferred | Also keep if already working in the source stack | Degrade |
|-------|-----------|--------------------------------------------------|---------|
| Terra Classic | Keplr (extension) + Simulated Wallet | Station, Cosmos WC mobile | Clear error if `signArbitrary` missing |
| BSC | Injected EIP-1193 (MetaMask) + Bridge-style WC | Coinbase connector if Bridge already has it | Wrong chain (not 56) → readable error |

## Rules of thumb

1. Port, then strip. Do not rewrite connect from docs-only memory.
2. One connected address at a time for a given flow (Terra **or** EVM). Do not auto-link chains.
3. Never prompt for a seed or private key.
4. Domain-separate signed payloads (`voting`, chain-id, purpose, proposal id). Reject cross-scheme blobs (ADR-36 posted as EVM and the reverse).
