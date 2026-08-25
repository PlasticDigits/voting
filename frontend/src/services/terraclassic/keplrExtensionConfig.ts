import { getKeplrLikeExtension } from '@/services/terraclassic/keplrLikeExtension'
import { getTerraChainSuggestion } from '@/services/terraclassic/terraChainSuggestion'
import type { ConnectedWallet } from '@goblinhunt/cosmes/wallet'
import { WalletName, WalletType } from '@goblinhunt/cosmes/wallet'

/** Session flag from Keplr `getKey().isNanoLedger` and/or cosmes `useAmino` (GitLab #567). */
export type LedgerAwareWallet = ConnectedWallet & {
  useAmino?: boolean
  isNanoLedger?: boolean
}

let sessionNanoLedger = false

/** Last detected Keplr Nano Ledger flag for this page session (connect + pre-sign). */
export function getSessionNanoLedger(): boolean {
  return sessionNanoLedger
}

export function setSessionNanoLedger(value: boolean): void {
  sessionNanoLedger = value
}

/**
 * True when this connected wallet is a Ledger behind Keplr (or an explicit Nano flag).
 * Do not treat Station/Cosmostation amino as Ledger UX — those wallets always amino (#208).
 */
export function walletIsNanoLedger(wallet: ConnectedWallet | null | undefined): boolean {
  if (!wallet) return false
  const w = wallet as LedgerAwareWallet
  if (w.isNanoLedger === true) return true
  return w.id === WalletName.KEPLR && w.useAmino === true
}

/** Keplr extension (and detected Ledger) get a long sign wait — never the 30s #173 broadcast cap. */
export function shouldApplyKeplrSignStallTimeout(wallet: ConnectedWallet): boolean {
  if (wallet.type !== WalletType.EXTENSION) return false
  if (wallet.id === WalletName.KEPLR) return true
  return walletIsNanoLedger(wallet)
}

/**
 * Persist `isNanoLedger` on the connected Keplr extension wallet for the session (GitLab #567).
 * Best-effort: missing `getKey` or rejection must not fail connect or sign.
 */
export async function rememberKeplrNanoLedgerFlag(wallet: ConnectedWallet): Promise<void> {
  if (wallet.id !== WalletName.KEPLR || wallet.type !== WalletType.EXTENSION) return
  const ext = getKeplrLikeExtension(WalletName.KEPLR)
  if (!ext?.getKey) return
  try {
    const key = await ext.getKey(wallet.chainId)
    if (typeof key?.isNanoLedger === 'boolean') {
      ;(wallet as LedgerAwareWallet).isNanoLedger = key.isNanoLedger
      setSessionNanoLedger(key.isNanoLedger)
    }
  } catch (err: unknown) {
    console.warn('[Wallet] Keplr getKey isNanoLedger failed (GitLab #567):', err)
  }
}

/**
 * Refresh Keplr Terra Classic chain metadata immediately before sign (parity with Station #127 / #208).
 * Failures warn and continue — never fail the swap. Uses only {@link getTerraChainSuggestion} (K567-3).
 */
export async function prepareKeplrExtensionForTerraClassicSign(wallet?: ConnectedWallet): Promise<void> {
  if (wallet && (wallet.id !== WalletName.KEPLR || wallet.type !== WalletType.EXTENSION)) {
    return
  }
  if (wallet) {
    await rememberKeplrNanoLedgerFlag(wallet)
    if (walletIsNanoLedger(wallet)) {
      setSessionNanoLedger(true)
    }
  }

  const ext = getKeplrLikeExtension(WalletName.KEPLR)
  if (!ext?.experimentalSuggestChain) return
  try {
    await ext.experimentalSuggestChain(getTerraChainSuggestion())
  } catch (err: unknown) {
    console.warn('[Wallet] Keplr experimentalSuggestChain before sign failed (GitLab #567):', err)
  }
}
