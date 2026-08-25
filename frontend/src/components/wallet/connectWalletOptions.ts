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
  return shouldOfferMobileExtensionWalletConnect(env.isMobileClient, env.keplrInjected)
}

export function shouldOfferStationWalletConnect(env: ConnectWalletOptionEnv): boolean {
  return shouldOfferMobileExtensionWalletConnect(env.isMobileClient, env.stationInjected)
}

export function shouldOfferCosmostationWalletConnect(env: ConnectWalletOptionEnv): boolean {
  return shouldOfferMobileExtensionWalletConnect(env.isMobileClient, env.cosmostationInjected)
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
  return [
    extensionOrWalletConnect('Station', WalletName.STATION, shouldOfferStationWalletConnect(env)),
    extensionOrWalletConnect('Keplr', WalletName.KEPLR, shouldOfferKeplrWalletConnect(env)),
    extensionOrWalletConnect('Cosmostation', WalletName.COSMOSTATION, shouldOfferCosmostationWalletConnect(env)),
    {
      name: 'LuncDash',
      walletName: WalletName.LUNCDASH,
      walletType: WalletType.WALLETCONNECT,
      connectionLabel: 'WalletConnect',
    },
    {
      name: 'Galaxy Station',
      walletName: WalletName.GALAXYSTATION,
      walletType: WalletType.WALLETCONNECT,
      connectionLabel: 'WalletConnect',
    },
  ]
}
