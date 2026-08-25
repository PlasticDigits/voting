import { useWalletStore } from '@/hooks/useWallet'
import { useWalletConnectPairingStore } from '@/hooks/useWalletConnectPairingStore'
import {
  isWalletConnectMobileClient,
  isWalletConnectPairingUri,
  WC_PAIRING_HOOK_KEY,
  type WalletConnectPairingHook,
  type WalletConnectPairingHookPayload,
} from '@/utils/walletConnectPairing'

/**
 * Registers the cosmes `QRCodeModal` intercept (GitLab #519).
 * Install once at boot so the hook exists before any WalletConnect `connect()`.
 */
export function installWalletConnectPairingHook(): () => void {
  const hook: WalletConnectPairingHook = {
    open(payload: WalletConnectPairingHookPayload) {
      if (!isWalletConnectPairingUri(payload.uri)) return false
      if (!isWalletConnectMobileClient()) return false
      useWalletStore.getState().setWalletModalOpen(false)
      useWalletConnectPairingStore.getState().open(payload)
      return true
    },
    close() {
      useWalletConnectPairingStore.getState().close()
    },
  }

  const bag = globalThis as typeof globalThis & Record<string, unknown>
  bag[WC_PAIRING_HOOK_KEY] = hook

  return () => {
    if (bag[WC_PAIRING_HOOK_KEY] === hook) {
      delete bag[WC_PAIRING_HOOK_KEY]
    }
  }
}
