import { DEFAULT_NETWORK, NETWORKS } from './constants'

export function getNetworkBadgeCopy(): {
  shortLabel: string
  fullLabel: string
  chainId: string
} {
  const chainId = NETWORKS[DEFAULT_NETWORK].terra.chainId
  if (DEFAULT_NETWORK === 'local') {
    return { shortLabel: 'Local', fullLabel: 'LocalTerra', chainId }
  }
  if (DEFAULT_NETWORK === 'testnet') {
    return { shortLabel: 'Testnet', fullLabel: 'Terra Classic Testnet', chainId }
  }
  return { shortLabel: 'Mainnet', fullLabel: 'Terra Classic', chainId }
}
