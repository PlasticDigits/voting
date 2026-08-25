import { useWalletStore } from '@/hooks/useWallet'
import { useEvmWalletStore } from '@/stores/evmWallet'
import type { Network } from '@plasticdigits/cl8y-clickwrap'
import type { VotingChain } from '@/utils/votingPayload'

export type ConnectedIdentity = {
  address: string | null
  chain: VotingChain | null
  legalNetwork: Network | null
  label: string
}

export function useConnectedIdentity(): ConnectedIdentity {
  const terraAddress = useWalletStore((s) => s.address)
  const evmAddress = useEvmWalletStore((s) => s.address)

  if (terraAddress) {
    return { address: terraAddress, chain: 'terra', legalNetwork: 'TerraClassic', label: 'Terra Classic' }
  }
  if (evmAddress) {
    return { address: evmAddress, chain: 'bsc', legalNetwork: 'EVM', label: 'BSC' }
  }
  return { address: null, chain: null, legalNetwork: null, label: '' }
}
