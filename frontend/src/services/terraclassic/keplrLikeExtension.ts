import { WalletName } from '@goblinhunt/cosmes/wallet'

/** Keplr-compatible `experimentalSuggestChain` / `getKey` surface (Station shim + Keplr family). */
export type KeplrKeyAccount = {
  name: string
  bech32Address: string
  pubKey: Uint8Array
  isNanoLedger: boolean
}

export type KeplrExperimentalSuggest = {
  experimentalSuggestChain?: (chainInfo: Record<string, unknown>) => Promise<void>
  defaultOptions?: { sign?: { preferNoSetFee?: boolean; preferNoSetMemo?: boolean } }
  getKey?: (chainId: string) => Promise<KeplrKeyAccount>
}

/**
 * Resolve the browser extension object that implements `experimentalSuggestChain`.
 * Station exposes this under `window.station.keplr` (GitLab #127: LocalTerra fee/gas step refresh).
 * For **signing** behaviour with Station, see the cosmes patch (`StationController` → amino, `KeplrExtension` `preferNoSetFee`), [`stationExtensionConfig.ts`](./stationExtensionConfig.ts), and [`skills/AGENTS_FRONTEND_STATION_SIGNING.md`](../../../../skills/AGENTS_FRONTEND_STATION_SIGNING.md) ([GitLab #208](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/208)).
 */
export function getKeplrLikeExtension(walletName: WalletName): KeplrExperimentalSuggest | undefined {
  if (typeof window === 'undefined') {
    return undefined
  }
  const w = window as unknown as {
    keplr?: KeplrExperimentalSuggest
    cosmostation?: { providers?: { keplr?: KeplrExperimentalSuggest } }
    station?: { keplr?: KeplrExperimentalSuggest }
  }
  switch (walletName) {
    case WalletName.STATION:
      return w.station?.keplr
    case WalletName.KEPLR:
      return w.keplr
    case WalletName.COSMOSTATION:
      return w.cosmostation?.providers?.keplr
    default:
      return undefined
  }
}
