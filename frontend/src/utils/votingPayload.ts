import {
  BSC_CHAIN,
  BSC_CHAIN_ID,
  DEFAULT_NETWORK,
  NETWORKS,
  SIGNATURE_TTL_SECS,
  TERRA_CHAIN,
  VOTING_APP,
} from './constants'

export type VotingChain = 'terra' | 'bsc'
export type VotingPurpose =
  | 'register'
  | 'propose'
  | 'draft'
  | 'amend'
  | 'comment'
  | 'analyze'
  | 'open_vote'
  | 'vote'
export type VoteChoice = 'for' | 'against' | 'abstain'

export type SignedPayload = {
  app: string
  chain: VotingChain
  chain_id: string
  purpose: VotingPurpose
  address: string
  issued_at: number
  expires_at: number
  title?: string
  body_hash?: string
  proposal_id?: string
  choice?: VoteChoice
  prev_body_hash?: string
}

export function votingChainId(chain: VotingChain): string {
  if (chain === TERRA_CHAIN) {
    return NETWORKS[DEFAULT_NETWORK].terra.chainId
  }
  return BSC_CHAIN_ID
}

export function inferVotingChain(address: string): VotingChain {
  return address.trim().startsWith('0x') || address.trim().startsWith('0X') ? BSC_CHAIN : TERRA_CHAIN
}

export function buildSignedPayload(input: {
  chain: VotingChain
  purpose: VotingPurpose
  address: string
  now?: number
  title?: string
  body_hash?: string
  proposal_id?: string
  choice?: VoteChoice
  prev_body_hash?: string
}): SignedPayload {
  const issued_at = input.now ?? Math.floor(Date.now() / 1000)
  const payload: SignedPayload = {
    app: VOTING_APP,
    chain: input.chain,
    chain_id: votingChainId(input.chain),
    purpose: input.purpose,
    address: input.address,
    issued_at,
    expires_at: issued_at + SIGNATURE_TTL_SECS,
  }
  if (input.title !== undefined) payload.title = input.title
  if (input.body_hash !== undefined) payload.body_hash = input.body_hash
  if (input.proposal_id !== undefined) payload.proposal_id = input.proposal_id
  if (input.choice !== undefined) payload.choice = input.choice
  if (input.prev_body_hash !== undefined) payload.prev_body_hash = input.prev_body_hash
  return payload
}

/** Canonical JSON matching operator-voting serde field order + skip_serializing_if. */
export function canonicalizePayload(payload: SignedPayload): string {
  const ordered: Record<string, unknown> = {
    app: payload.app,
    chain: payload.chain,
    chain_id: payload.chain_id,
    purpose: payload.purpose,
    address: payload.address,
    issued_at: payload.issued_at,
    expires_at: payload.expires_at,
  }
  if (payload.title !== undefined) ordered.title = payload.title
  if (payload.body_hash !== undefined) ordered.body_hash = payload.body_hash
  if (payload.proposal_id !== undefined) ordered.proposal_id = payload.proposal_id
  if (payload.choice !== undefined) ordered.choice = payload.choice
  if (payload.prev_body_hash !== undefined) ordered.prev_body_hash = payload.prev_body_hash
  return JSON.stringify(ordered)
}

export function assertPurposeSeparation(payload: SignedPayload, expected: VotingPurpose): void {
  if (payload.purpose !== expected) {
    throw new Error(`purpose ${payload.purpose} cannot be used as ${expected}`)
  }
  if (payload.app !== VOTING_APP) {
    throw new Error('wrong signing app domain')
  }
}

export function canPropose(balanceRaw: bigint | string, minRaw = 1000n * 10n ** 18n): boolean {
  const value = typeof balanceRaw === 'string' ? BigInt(balanceRaw) : balanceRaw
  return value >= minRaw
}
