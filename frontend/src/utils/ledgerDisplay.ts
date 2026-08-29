import type { BalanceResponse } from '@/services/operatorVoting'

/**
 * Ledger `0` is not a live LCD/`balanceOf` read. Only show an amount after
 * `voting_registrations` exists (issue #9). A registered true-zero is shown.
 */
export function snapshotBalanceRaw(bal: BalanceResponse | null | undefined): string | null {
  if (!bal) return null
  if (bal.registered === false) return null
  if (bal.registered === true) return bal.balance
  return null
}

export function isLedgerPending(bal: BalanceResponse | null | undefined): boolean {
  return Boolean(bal && !bal.registered && bal.pending)
}
