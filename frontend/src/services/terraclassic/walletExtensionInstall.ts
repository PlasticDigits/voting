import { WalletName } from '@goblinhunt/cosmes/wallet'
import { getKeplrLikeExtension } from '@/services/terraclassic/keplrLikeExtension'

/**
 * Detects whether a browser extension for this wallet name is injected on `window`.
 * Used by the connect modal for **Ready** when detected, dimmed row + **Install** link when not ([GitLab #139](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/139), [#160](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/160)).
 *
 * **Invariants (must stay aligned with `getKeplrLikeExtension`):**
 * - **Station:** `'station' in window` — same signal as legacy `isStationInstalled` (extension injected; shim may still fail at connect).
 * - **Keplr:** `window.keplr` truthy.
 * - **Cosmostation:** same object `getKeplrLikeExtension` reads (`window.cosmostation?.providers?.keplr`).
 * - **WalletConnect-only names** (e.g. LuncDash, Galaxy Station): returns `true` so the modal does not treat them as missing extensions.
 * - **Station / Cosmostation WalletConnect** on mobile Chrome ([#566](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/566)): the Connect list uses `WalletType.WALLETCONNECT`, so this detector is not consulted for those rows.
 * - **Leap:** not offered in the UI ([GitLab #159](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/159)); do not add an install URL or modal row while the vendor page is sunset.
 */
export function isBrowserWalletExtensionDetected(walletName: WalletName): boolean {
  if (typeof window === 'undefined') {
    return false
  }

  switch (walletName) {
    case WalletName.STATION:
      return 'station' in window
    case WalletName.KEPLR:
      return !!(window as unknown as { keplr?: unknown }).keplr
    case WalletName.COSMOSTATION:
      return !!getKeplrLikeExtension(walletName)
    case WalletName.LUNCDASH:
    case WalletName.GALAXYSTATION:
      return true
    default:
      return false
  }
}

/** Official download / setup pages (not Chrome IDs) — stable landing URLs for “Install”. */
export const WALLET_EXTENSION_INSTALL_URL: Partial<Record<WalletName, string>> = {
  [WalletName.STATION]: 'https://setup.money/',
  [WalletName.KEPLR]: 'https://www.keplr.app/download',
  [WalletName.COSMOSTATION]: 'https://wallet.cosmostation.io/',
}
