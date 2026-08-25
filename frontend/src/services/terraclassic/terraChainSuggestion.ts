import { DEFAULT_NETWORK, NETWORKS, effectiveGasPriceUluna } from '@/utils/constants'

const TERRA_CLASSIC_BECH32 = {
  bech32PrefixAccAddr: 'terra',
  bech32PrefixAccPub: 'terrapub',
  bech32PrefixValAddr: 'terravaloper',
  bech32PrefixValPub: 'terravaloperpub',
  bech32PrefixConsAddr: 'terravalcons',
  bech32PrefixConsPub: 'terravalconspub',
}

/** Keplr / Station `experimentalSuggestChain` metadata for the active Terra Classic network. */
export function getTerraChainSuggestion(): Record<string, unknown> {
  const config = NETWORKS[DEFAULT_NETWORK].terra
  return {
    chainId: config.chainId,
    chainName: DEFAULT_NETWORK === 'local' ? 'LocalTerra' : `Terra Classic (${DEFAULT_NETWORK})`,
    rpc: config.rpc,
    rest: config.lcd,
    bip44: { coinType: 330 },
    bech32Config: TERRA_CLASSIC_BECH32,
    currencies: [
      { coinDenom: 'LUNC', coinMinimalDenom: 'uluna', coinDecimals: 6 },
      { coinDenom: 'USTC', coinMinimalDenom: 'uusd', coinDecimals: 6 },
    ],
    feeCurrencies: [
      {
        coinDenom: 'LUNC',
        coinMinimalDenom: 'uluna',
        coinDecimals: 6,
        gasPriceStep: { low: effectiveGasPriceUluna(), average: effectiveGasPriceUluna(), high: 50 },
      },
    ],
    stakeCurrency: { coinDenom: 'LUNC', coinMinimalDenom: 'uluna', coinDecimals: 6 },
  }
}
