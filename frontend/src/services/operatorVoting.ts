import { inferVotingChain, type VoteChoice, type VotingChain } from '@/utils/votingPayload'
import type { AnalysisSections, ProposalSections } from '@/utils/proposalSections'

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
  summary?: string | null
  terra_height: number | null
  bsc_block: number | null
  created_at: string
  opened_at?: string | null
  status: string
  tally: { choice: string; weight: string }[] | Record<string, string>
}

export type ProposalComment = {
  id: string
  chain: string
  wallet_address: string
  body_html: string
  created_at: string
}

export type ProposalAnalysis = {
  id: string
  chain: string
  wallet_address: string
  body_html: string
  source: string
  created_at: string
  sections: AnalysisSections
  machine_generated?: boolean
}

export type ProposalDetail = ProposalListItem & {
  body_html: string
  sections?: ProposalSections | null
  comments?: ProposalComment[]
  analysis?: ProposalAnalysis[]
  advisory: boolean
}

export type BalanceResponse = {
  chain: string
  address: string
  height: number
  as_of_height?: number
  balance: string
  registered: boolean
  pending: boolean
  initial_balance?: string | null
}

export type ChainRegistrationRow = {
  chain: string
  wallet_address?: string
  address?: string
  registered_at_height: number
  status: string
}

export type RegistrationLookup = {
  terra?: ChainRegistrationRow | null
  bsc?: ChainRegistrationRow | null
  pending?: { terra?: boolean; bsc?: boolean }
}

export type RegistrationResponse = {
  ok: boolean
  pending?: boolean
  signature_id?: string
  registration?: { chain: string; address: string; registered_at_height: number; status: string } | null
}

export const REGISTER_POLL_INTERVAL_MS = 1_000
export const REGISTER_POLL_TIMEOUT_MS = 30_000

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

export async function getRegistration(address: string): Promise<RegistrationLookup | null> {
  const res = await fetch(`${operatorVotingUrl()}/v1/registration/${encodeURIComponent(address)}`)
  if (res.status === 404) return null
  if (!res.ok) throw new Error(`registration lookup failed: ${res.status}`)
  return res.json() as Promise<RegistrationLookup>
}

export function chainRegistration(
  lookup: RegistrationLookup | null,
  chain: VotingChain
): ChainRegistrationRow | null {
  return lookup?.[chain] ?? null
}

export function isRegistrationPending(lookup: RegistrationLookup | null, chain: VotingChain): boolean {
  return Boolean(lookup?.pending?.[chain])
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Poll until `voting_registrations` exists for this address/chain. POST `/v1/register` is only an intent. */
export async function waitForLedgerRegistration(
  address: string,
  chain: VotingChain,
  opts?: {
    timeoutMs?: number
    intervalMs?: number
    now?: () => number
    sleep?: (ms: number) => Promise<void>
    lookup?: (address: string) => Promise<RegistrationLookup | null>
  }
): Promise<RegistrationLookup> {
  const timeoutMs = opts?.timeoutMs ?? REGISTER_POLL_TIMEOUT_MS
  const intervalMs = opts?.intervalMs ?? REGISTER_POLL_INTERVAL_MS
  const now = opts?.now ?? Date.now
  const pause = opts?.sleep ?? sleep
  const lookup = opts?.lookup ?? getRegistration
  const deadline = now() + timeoutMs
  while (now() < deadline) {
    const last = await lookup(address)
    if (last && chainRegistration(last, chain)) return last
    await pause(intervalMs)
  }
  throw new Error('Registration snapshot is still pending. Wait for the ledger, then retry.')
}

export async function listProposals(): Promise<ProposalListItem[]> {
  return api('/v1/proposals')
}

export async function getProposal(id: string): Promise<ProposalDetail> {
  return api(`/v1/proposals/${encodeURIComponent(id)}`)
}

export async function createProposal(
  req: SignedRequest & { title: string; sections: ProposalSections }
): Promise<{ id: string; status: string; terra_height: number | null; bsc_block: number | null }> {
  return api('/v1/proposals', { method: 'POST', body: JSON.stringify(req) })
}

export async function amendProposal(
  id: string,
  req: SignedRequest & { title: string; sections: ProposalSections }
): Promise<{ ok: boolean; body_hash: string }> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/sections`, {
    method: 'PUT',
    body: JSON.stringify(req),
  })
}

export async function postComment(
  id: string,
  req: SignedRequest & { body_html: string }
): Promise<{ id: string }> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/comments`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function postAnalysis(
  id: string,
  req: SignedRequest & { sections: AnalysisSections }
): Promise<{ id: string; source: string }> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/analysis`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function openProposal(
  id: string,
  req: SignedRequest
): Promise<{ ok: boolean; status: string; terra_height: number; bsc_block: number }> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/open`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
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
