import { getKeplrLikeExtension } from '@/services/terraclassic/keplrLikeExtension'
import {
  ensureStationLocalNetworkRegistered,
  shouldUseStationNativeLocalNetwork,
} from '@/services/terraclassic/stationNativeNetwork'
import { getTerraChainSuggestion } from '@/services/terraclassic/terraChainSuggestion'
import { DEFAULT_NETWORK, NETWORKS, effectiveGasPriceUluna } from '@/utils/constants'
import type { ConnectedWallet } from '@goblinhunt/cosmes/wallet'
import { WalletName } from '@goblinhunt/cosmes/wallet'

/**
 * Station exposes a Keplr-compatible shim at `window.station.keplr`.
 * Configure sign options before connect and before every broadcast (GitLab #127, #208).
 */
export function applyStationKeplrShimSignDefaults(): void {
  const stationKeplr = getKeplrLikeExtension(WalletName.STATION)
  if (!stationKeplr) {
    return
  }
  stationKeplr.defaultOptions = {
    sign: {
      preferNoSetFee: true,
      preferNoSetMemo: true,
    },
  }
}

async function suggestStationChainMetadata(): Promise<void> {
  const ext = getKeplrLikeExtension(WalletName.STATION)
  if (ext?.experimentalSuggestChain) {
    await ext.experimentalSuggestChain(getTerraChainSuggestion())
  }
}

/**
 * Refresh Station chain gas metadata and shim defaults immediately before sign/broadcast.
 * Station on LocalTerra can rewrite fees from a stale ~0.015 uluna/gas step even when the dApp
 * supplies the correct amino fee (swap repro: ~12,600 vs ~23,793,000 uluna — GitLab #127).
 */
export async function prepareStationExtensionForTerraClassicSign(wallet?: ConnectedWallet): Promise<void> {
  applyStationKeplrShimSignDefaults()

  const gasPrice = { amount: String(effectiveGasPriceUluna()), denom: 'uluna' as const }
  wallet?.setGasPrice?.(gasPrice)

  if (DEFAULT_NETWORK === 'local') {
    const { lcd, chainId } = NETWORKS.local.terra
    if (shouldUseStationNativeLocalNetwork()) {
      try {
        await ensureStationLocalNetworkRegistered(lcd, chainId)
      } catch (err: unknown) {
        console.warn('[Wallet] Station addNetwork refresh before sign failed (GitLab #127):', err)
      }
    } else {
      try {
        await suggestStationChainMetadata()
      } catch (err: unknown) {
        console.warn('[Wallet] Station experimentalSuggestChain before sign failed (GitLab #127):', err)
      }
    }
    return
  }

  try {
    await suggestStationChainMetadata()
  } catch (err: unknown) {
    console.warn('[Wallet] Station experimentalSuggestChain before sign failed (GitLab #208):', err)
  }
}
