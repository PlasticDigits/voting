import { WalletName } from '@goblinhunt/cosmes/wallet'

/**
 * Local Connect Wallet logos (#490).
 * Provenance: `frontend-dapp/public/wallets/PROVENANCE.md`
 * Assets are static under `/wallets/*` — never hotlink CDNs.
 *
 * Only production Terra Classic wallets listed in `WalletModal` have icons.
 * Other `WalletName` enum members (Leap, Compass, …) stay undefined — Leap must
 * not reappear in the modal (#159).
 */
const WALLET_ICON_SRC: Partial<Record<WalletName, string>> = {
  [WalletName.STATION]: '/wallets/station.svg',
  [WalletName.KEPLR]: '/wallets/keplr.svg',
  [WalletName.COSMOSTATION]: '/wallets/cosmostation.svg',
  [WalletName.LUNCDASH]: '/wallets/luncdash.png',
  [WalletName.GALAXYSTATION]: '/wallets/galaxy-station.png',
}

/** Production wallets shown in Connect Wallet (order matches modal). */
export const PRODUCTION_WALLET_NAMES: readonly WalletName[] = [
  WalletName.STATION,
  WalletName.KEPLR,
  WalletName.COSMOSTATION,
  WalletName.LUNCDASH,
  WalletName.GALAXYSTATION,
] as const

/** Dev Simulated Wallet glyph — original mark, not a vendor trademark. */
export const SIMULATED_WALLET_ICON_SRC = '/wallets/simulated.svg'

export function walletIconSrc(walletName: WalletName): string | undefined {
  return WALLET_ICON_SRC[walletName]
}

export function productionWalletIconSrcs(): string[] {
  return PRODUCTION_WALLET_NAMES.map((name) => WALLET_ICON_SRC[name]).filter(
    (src): src is string => typeof src === 'string' && src.length > 0
  )
}
