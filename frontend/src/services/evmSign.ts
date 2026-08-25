import { getAccount, signMessage } from 'wagmi/actions'
import { config } from '@/lib/wagmi'

/** EIP-191 `personal_sign` for voting payloads. */
export async function signEvmVotingPayload(address: string, payloadString: string): Promise<string> {
  const account = getAccount(config)
  if (account.address && account.address.toLowerCase() !== address.toLowerCase()) {
    throw new Error('Connected EVM address does not match the signing address')
  }
  if (account.chainId && account.chainId !== 56 && account.chainId !== 31337) {
    throw new Error(`Wrong EVM network (${account.chainId}). Switch to BSC (chain id 56) to vote.`)
  }
  return signMessage(config, { account: address as `0x${string}`, message: payloadString })
}
