import { getNetworkBadgeCopy } from '@/utils/networkDisplay'

/** Substrings from Station / cosmes when the extension is on another chain (GitLab #207). */
const WRONG_NETWORK_HINT =
  /not available on station|chain is not available|requested chain is not|failed to get public key for|no wallets connected/i

const NOT_INSTALLED_HINT =
  /extension is not installed|wallet extension is not installed|install the station extension|install the keplr extension/i

export function isWalletWrongNetworkError(message: string): boolean {
  const m = message.trim()
  if (!m) return false
  if (NOT_INSTALLED_HINT.test(m)) return false
  return WRONG_NETWORK_HINT.test(m)
}

export function isWalletExtensionNotInstalledError(message: string): boolean {
  return NOT_INSTALLED_HINT.test(message.trim())
}

export function buildWrongNetworkConnectError(walletLabel: string): string {
  const { fullLabel, chainId } = getNetworkBadgeCopy()
  return (
    `${walletLabel} is installed but on the wrong network for this app. ` +
    `Open the ${walletLabel} extension, switch to ${fullLabel} (chain ID ${chainId}), unlock your wallet, then connect again.`
  )
}
