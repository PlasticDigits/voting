import { inferVotingChain, type VoteChoice, type VotingChain } from '@/utils/votingPayload'

export type SignedRequest = {
  chain: VotingChain
  address: string
  payload: string
  signature: string
  pubkey?: string
}

export type ProposalListItem = {
  id: string
  chain: string
  proposer: string
  title: string
  terra_height: number
  bsc_block: number
  created_at: string
  status: string
  tally: { choice: string; weight: string }[] | Record<string, string>
}

export type ProposalDetail = ProposalListItem & {
  body_html: string
  advisory: boolean
}

export type BalanceResponse = {
  chain: string
  address: string
  height: number
  balance: string
}

export type RegistrationResponse = {
  ok: boolean
  pending?: boolean
  signature_id?: string
  registration?: { chain: string; address: string; registered_at_height: number; status: string } | null
}

function operatorVotingUrl(): string {
  const base = import.meta.env.VITE_OPERATOR_VOTING_URL?.trim()
  if (!base) {
    throw new Error('VITE_OPERATOR_VOTING_URL is not set')
  }
  return base.replace(/\/$/, '')
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${operatorVotingUrl()}${path}`, {
    ...init,
    headers: {
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      ...init?.headers,
    },
  })
  if (!res.ok) {
    const body = (await res.json().catch(() => ({}))) as { error?: string; message?: string }
    throw new Error(body.error ?? body.message ?? `${res.status} ${res.statusText}`)
  }
  return res.json() as Promise<T>
}

export async function registerWallet(req: SignedRequest): Promise<RegistrationResponse> {
  return api('/v1/register', { method: 'POST', body: JSON.stringify(req) })
}

export async function getRegistration(address: string): Promise<Record<string, unknown> | null> {
  const res = await fetch(`${operatorVotingUrl()}/v1/registration/${encodeURIComponent(address)}`)
  if (res.status === 404) return null
  if (!res.ok) throw new Error(`registration lookup failed: ${res.status}`)
  return res.json() as Promise<Record<string, unknown>>
}

export async function listProposals(): Promise<ProposalListItem[]> {
  return api('/v1/proposals')
}

export async function getProposal(id: string): Promise<ProposalDetail> {
  return api(`/v1/proposals/${encodeURIComponent(id)}`)
}

export async function createProposal(
  req: SignedRequest & { title: string; body_html: string }
): Promise<{ id: string; terra_height: number; bsc_block: number }> {
  return api('/v1/proposals', { method: 'POST', body: JSON.stringify(req) })
}

export async function castVote(
  id: string,
  req: SignedRequest & { choice: VoteChoice }
): Promise<{ ok: boolean; weight: string }> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/votes`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function getBalance(
  address: string,
  chain: VotingChain = inferVotingChain(address)
): Promise<BalanceResponse> {
  const q = new URLSearchParams({ chain })
  return api(`/v1/balances/${encodeURIComponent(address)}?${q}`)
}
