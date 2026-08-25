import { DEFAULT_NETWORK, NETWORKS } from '@/utils/constants'
import { getConnectedWallet } from '@/services/terraclassic/wallet'

export type TerraSignature = {
  signature: string
  pubkey: string
}

/**
 * ADR-36 arbitrary sign for voting payloads.
 * Prefer `window.keplr.signArbitrary`. Simulated / WC wallets use cosmes `signArbitrary`.
 */
export async function signTerraVotingPayload(address: string, payloadString: string): Promise<TerraSignature> {
  const chainId = NETWORKS[DEFAULT_NETWORK].terra.chainId
  const keplr = window.keplr
  if (keplr?.signArbitrary) {
    const result = await keplr.signArbitrary(chainId, address, payloadString)
    return {
      signature: result.signature,
      pubkey: result.pub_key.value,
    }
  }

  const wallet = getConnectedWallet()
  if (wallet && typeof wallet.signArbitrary === 'function') {
    const result = await wallet.signArbitrary(payloadString)
    return {
      signature: result.signature,
      pubkey: result.pubKey,
    }
  }

  throw new Error(
    'This wallet cannot signArbitrary (ADR-36). Install or unlock Keplr, or use Simulated Wallet in DEV_MODE.'
  )
}
