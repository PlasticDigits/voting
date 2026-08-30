import { signEvmVotingPayload } from '@/services/evmSign'
import { signTerraVotingPayload } from '@/services/terraSign'
import type { SignedRequest } from '@/services/operatorVoting'
import {
  assertPurposeSeparation,
  buildSignedPayload,
  canonicalizePayload,
  type VoteChoice,
  type VotingChain,
  type VotingPurpose,
} from '@/utils/votingPayload'

export async function signVotingRequest(input: {
  chain: VotingChain
  address: string
  purpose: VotingPurpose
  title?: string
  body_hash?: string
  proposal_id?: string
  choice?: VoteChoice
  prev_body_hash?: string
}): Promise<SignedRequest> {
  const payload = buildSignedPayload(input)
  assertPurposeSeparation(payload, input.purpose)
  const payloadString = canonicalizePayload(payload)
  if (input.chain === 'terra') {
    const { signature, pubkey } = await signTerraVotingPayload(input.address, payloadString)
    return { chain: 'terra', address: input.address, payload: payloadString, signature, pubkey }
  }
  const signature = await signEvmVotingPayload(input.address, payloadString)
  return { chain: 'bsc', address: input.address, payload: payloadString, signature }
}
