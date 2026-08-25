import { effectiveGasPriceUluna } from '@/utils/constants'

/** Flat network descriptor for Station extension `addNetwork` / `hasNetwork` (wallet-provider shape). */
export type StationNativeNetworkInfo = {
  name: string
  chainID: string
  lcd: string
  prefix?: string
  coinType?: string
  baseAsset?: string
  gasAdjustment?: number
  gasPrices?: Record<string, number>
}

export type StationNativeApi = {
  addNetwork?: (network: StationNativeNetworkInfo) => Promise<boolean>
  hasNetwork?: (network: Omit<StationNativeNetworkInfo, 'name'>) => Promise<boolean>
}

export function buildStationLocalNetworkInfo(lcd: string, chainId = 'localterra'): StationNativeNetworkInfo {
  return {
    name: 'LocalTerra',
    chainID: chainId,
    lcd,
    prefix: 'terra',
    coinType: '330',
    baseAsset: 'uluna',
    gasAdjustment: 1.75,
    gasPrices: { uluna: effectiveGasPriceUluna() },
  }
}

function getStationNativeApi(): StationNativeApi | undefined {
  if (typeof window === 'undefined') {
    return undefined
  }
  return (window as Window & { station?: StationNativeApi }).station
}

/**
 * Register LocalTerra on the Station extension via its native network API.
 * New Station builds reject `localterra` on `station.keplr.experimentalSuggestChain` (GitLab #207);
 * `addNetwork` is the supported path for custom / local LCD endpoints.
 */
export async function ensureStationLocalNetworkRegistered(
  lcd: string,
  chainId = 'localterra'
): Promise<'registered' | 'updated' | 'skipped'> {
  const station = getStationNativeApi()
  if (!station?.addNetwork) {
    return 'skipped'
  }

  const network = buildStationLocalNetworkInfo(lcd, chainId)
  const networkKey = { chainID: chainId, lcd }

  try {
    let existed = false
    if (station.hasNetwork) {
      existed = await station.hasNetwork(networkKey)
    }
    // Always call addNetwork so Station refreshes gasPrices (GitLab #127: stale ~0.015 uluna/gas
    // survives reconnect when hasNetwork short-circuited registration).
    await station.addNetwork(network)
    return existed ? 'updated' : 'registered'
  } catch (err: unknown) {
    console.warn('[Wallet] Station addNetwork for LocalTerra failed (GitLab #207, #127):', err)
    throw err
  }
}

/** Keplr shim on new Station rejects localterra for experimentalSuggestChain — use native API instead. */
export function shouldUseStationNativeLocalNetwork(): boolean {
  return getStationNativeApi()?.addNetwork != null
}
