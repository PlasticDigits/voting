import { WalletName, WalletType } from '@goblinhunt/cosmes/wallet'

export type ConnectWalletOption = {
  name: string
  walletName: WalletName
  walletType: WalletType
  connectionLabel: string
}

export type ConnectWalletOptionEnv = {
  isMobileClient: boolean
  keplrInjected: boolean
  stationInjected: boolean
  cosmostationInjected: boolean
  /**
   * WalletConnect v2 project id is present (`VITE_WC_PROJECT_ID`).
   * Production builds fail closed without it (#12). When false (local misconfig),
   * hide Galaxy Station and Keplr/Cosmostation WC rows so Connecting... cannot hang.
   * Station / LuncDash WC v1 still work without a Cloud project id.
   */
  walletConnectConfigured?: boolean
}

function walletConnectV2Configured(env: ConnectWalletOptionEnv): boolean {
  return env.walletConnectConfigured !== false
}

/**
 * Connect list rows (GitLab #554 / #566).
 *
 * Mobile Chrome without the matching extension offers Keplr / Station /
 * Cosmostation via WalletConnect — not an Install-only desktop extension row.
 * Injected extensions (in-app browser) stay Extension (**WC-M7** / **WC-M10**).
 * Leap stays absent ([#159](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/159)).
 */
export function shouldOfferMobileExtensionWalletConnect(isMobileClient: boolean, extensionInjected: boolean): boolean {
  return isMobileClient && !extensionInjected
}

export function shouldOfferKeplrWalletConnect(env: ConnectWalletOptionEnv): boolean {
  return (
    walletConnectV2Configured(env) &&
    shouldOfferMobileExtensionWalletConnect(env.isMobileClient, env.keplrInjected)
  )
}

export function shouldOfferStationWalletConnect(env: ConnectWalletOptionEnv): boolean {
  return shouldOfferMobileExtensionWalletConnect(env.isMobileClient, env.stationInjected)
}

export function shouldOfferCosmostationWalletConnect(env: ConnectWalletOptionEnv): boolean {
  return (
    walletConnectV2Configured(env) &&
    shouldOfferMobileExtensionWalletConnect(env.isMobileClient, env.cosmostationInjected)
  )
}

function extensionOrWalletConnect(
  name: string,
  walletName: WalletName,
  offerWalletConnect: boolean
): ConnectWalletOption {
  if (offerWalletConnect) {
    return {
      name,
      walletName,
      walletType: WalletType.WALLETCONNECT,
      connectionLabel: 'WalletConnect',
    }
  }
  return {
    name,
    walletName,
    walletType: WalletType.EXTENSION,
    connectionLabel: 'Extension',
  }
}

export function resolveConnectWalletOptions(env: ConnectWalletOptionEnv): ConnectWalletOption[] {
  const rows: ConnectWalletOption[] = [
    extensionOrWalletConnect('Station', WalletName.STATION, shouldOfferStationWalletConnect(env)),
    extensionOrWalletConnect('Keplr', WalletName.KEPLR, shouldOfferKeplrWalletConnect(env)),
    extensionOrWalletConnect('Cosmostation', WalletName.COSMOSTATION, shouldOfferCosmostationWalletConnect(env)),
    {
      name: 'LuncDash',
      walletName: WalletName.LUNCDASH,
      walletType: WalletType.WALLETCONNECT,
      connectionLabel: 'WalletConnect',
    },
  ]
  if (walletConnectV2Configured(env)) {
    rows.push({
      name: 'Galaxy Station',
      walletName: WalletName.GALAXYSTATION,
      walletType: WalletType.WALLETCONNECT,
      connectionLabel: 'WalletConnect',
    })
  }
  return rows
}
