import { useSyncExternalStore } from 'react'
import { WalletName } from '@goblinhunt/cosmes/wallet'
import { isBrowserWalletExtensionDetected } from '@/services/terraclassic/walletExtensionInstall'

const EXTENSION_WALLETS: WalletName[] = [WalletName.STATION, WalletName.KEPLR, WalletName.COSMOSTATION]

function readSnapshot(): Map<WalletName, boolean> {
  const m = new Map<WalletName, boolean>()
  for (const id of EXTENSION_WALLETS) {
    m.set(id, isBrowserWalletExtensionDetected(id))
  }
  return m
}

function snapshotsEqual(a: Map<WalletName, boolean>, b: Map<WalletName, boolean>): boolean {
  for (const id of EXTENSION_WALLETS) {
    if (a.get(id) !== b.get(id)) return false
  }
  return true
}

let cachedSnapshot: Map<WalletName, boolean> | null = null

function getCachedSnapshot(): Map<WalletName, boolean> {
  const next = readSnapshot()
  if (cachedSnapshot && snapshotsEqual(cachedSnapshot, next)) {
    return cachedSnapshot
  }
  cachedSnapshot = next
  return next
}

function subscribe(onChange: () => void): () => void {
  const notify = () => {
    onChange()
  }
  if (typeof window === 'undefined') {
    return () => {}
  }
  window.addEventListener('focus', notify)
  document.addEventListener('visibilitychange', notify)
  return () => {
    window.removeEventListener('focus', notify)
    document.removeEventListener('visibilitychange', notify)
  }
}

const SERVER_EXTENSION_SNAPSHOT: Map<WalletName, boolean> = (() => {
  const m = new Map<WalletName, boolean>()
  for (const id of EXTENSION_WALLETS) {
    m.set(id, false)
  }
  return m
})()

function getServerSnapshot(): Map<WalletName, boolean> {
  return SERVER_EXTENSION_SNAPSHOT
}

/**
 * Live map of extension install detection for modal rows. Re-reads on window focus / visibility
 * so returning from an install flow updates **Ready** / row state without a full reload.
 */
export function useWalletExtensionInstallSnapshot(): Map<WalletName, boolean> {
  return useSyncExternalStore(subscribe, getCachedSnapshot, getServerSnapshot)
}
