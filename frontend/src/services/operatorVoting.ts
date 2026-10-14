import { sha256Hex } from '@/utils/sanitizeProposalHtml'
import { inferVotingChain, type VoteChoice, type VotingChain } from '@/utils/votingPayload'
import type { AnalysisSections, ProposalSections } from '@/utils/proposalSections'

type Schemas = import('./generated/votingApi').components['schemas']
export type SignedRequest = Schemas['SignedRequest']
export type ProposalListItem = Schemas['ProposalListItem']
export type ProposalComment = Schemas['Comment']
export type ProposalAnalysis = Schemas['Analysis']
export type ProposalDetail = Schemas['ProposalDetail']
export type FrozenWeightResponse = Schemas['FrozenWeightResponse']
export type RecordedVote = Schemas['Vote']
export type BalanceResponse = Schemas['BalanceResponse']
export type ChainRegistrationRow = Schemas['RegistrationRow']
export type RegistrationLookup = Schemas['RegistrationLookup']
export type RegistrationResponse = Schemas['RegistrationResponse']

export const REGISTER_POLL_INTERVAL_MS = 1_000
export const REGISTER_POLL_MAX_INTERVAL_MS = 8_000
/** Default 120s: ledger poll is 4s and each LCD URL times out at 20s (#14). */
export const REGISTER_POLL_TIMEOUT_MS = envPositiveMs(
  import.meta.env.VITE_REGISTER_POLL_TIMEOUT_MS,
  120_000,
)
export const REGISTER_PENDING_TIMEOUT_MESSAGE =
  'Registration snapshot is still pending. Wait for the ledger, then retry.'

function envPositiveMs(raw: unknown, fallback: number): number {
  const n = Number(raw)
  return Number.isFinite(n) && n > 0 ? n : fallback
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

async function getOptional<T>(path: string): Promise<T | null> {
  const response = await fetch(`${operatorVotingUrl()}${path}`, {
    headers: { Accept: 'application/json' },
  })
  if (response.status === 404) return null
  if (!response.ok) throw new Error(`lookup failed: ${response.status}`)
  return response.json() as Promise<T>
}

export function getRegistration(address: string): Promise<RegistrationLookup | null> {
  return getOptional(`/v1/registration/${encodeURIComponent(address)}`)
}

export function getRecordedVote(
  id: string,
  address: string,
  chain: VotingChain,
): Promise<RecordedVote | null> {
  return getOptional(
    `/v1/proposals/${encodeURIComponent(id)}/votes/${encodeURIComponent(address)}?${new URLSearchParams({ chain })}`,
  )
}

export function chainRegistration(
  lookup: RegistrationLookup | null,
  chain: VotingChain,
): ChainRegistrationRow | null {
  return lookup?.[chain] ?? null
}

export function isRegistrationPending(
  lookup: RegistrationLookup | null,
  chain: VotingChain,
): boolean {
  return Boolean(lookup?.pending?.[chain])
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export function isPollAborted(err: unknown): boolean {
  return err instanceof DOMException
    ? err.name === 'AbortError'
    : err instanceof Error && err.name === 'AbortError'
}

/** Poll until `voting_registrations` exists for this address/chain. POST `/v1/register` is only an intent. */
export async function waitForLedgerRegistration(
  address: string,
  chain: VotingChain,
  opts?: {
    timeoutMs?: number
    intervalMs?: number
    maxIntervalMs?: number
    now?: () => number
    sleep?: (ms: number) => Promise<void>
    lookup?: (address: string) => Promise<RegistrationLookup | null>
    signal?: AbortSignal
    onAttempt?: (at: number) => void
  },
): Promise<RegistrationLookup> {
  const timeoutMs = opts?.timeoutMs ?? REGISTER_POLL_TIMEOUT_MS
  const maxIntervalMs = opts?.maxIntervalMs ?? REGISTER_POLL_MAX_INTERVAL_MS
  let intervalMs = opts?.intervalMs ?? REGISTER_POLL_INTERVAL_MS
  const now = opts?.now ?? Date.now
  const pause = opts?.sleep ?? sleep
  const lookup = opts?.lookup ?? getRegistration
  const deadline = now() + timeoutMs
  while (now() < deadline) {
    if (opts?.signal?.aborted) {
      throw new DOMException('Registration poll aborted', 'AbortError')
    }
    const last = await lookup(address)
    opts?.onAttempt?.(now())
    if (last && chainRegistration(last, chain)) return last
    const remaining = deadline - now()
    if (remaining <= 0) break
    await pause(Math.min(intervalMs, remaining))
    intervalMs = Math.min(intervalMs * 2, maxIntervalMs)
  }
  throw new Error(REGISTER_PENDING_TIMEOUT_MESSAGE)
}

export async function listProposals(): Promise<ProposalListItem[]> {
  return api('/v1/proposals')
}

export async function getProposal(id: string): Promise<ProposalDetail> {
  return api(`/v1/proposals/${encodeURIComponent(id)}`)
}

export async function createProposal(
  req: SignedRequest & { title: string; body_sections: ProposalSections },
): Promise<Schemas['DraftResponse']> {
  return api('/v1/proposals', { method: 'POST', body: JSON.stringify(req) })
}

export async function amendProposal(
  id: string,
  req: SignedRequest & { title: string; body_sections: ProposalSections },
): Promise<Schemas['AmendResponse']> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/sections`, {
    method: 'PUT',
    body: JSON.stringify(req),
  })
}

export async function postComment(
  id: string,
  req: SignedRequest & { body_html: string },
): Promise<Schemas['CommentResponse']> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/comments`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function postAnalysis(
  id: string,
  req: SignedRequest & { sections: AnalysisSections },
): Promise<Schemas['AnalysisResponse']> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/analysis`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function openProposal(
  id: string,
  req: SignedRequest,
): Promise<Schemas['OpenResponse']> {
  return api(`/v1/proposals/${encodeURIComponent(id)}/open`, {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export async function castVote(
  id: string,
  req: SignedRequest & { choice: VoteChoice },
): Promise<Schemas['VoteResponse']> {
  const path = `/v1/proposals/${encodeURIComponent(id)}/votes`
  const init = { method: 'POST', body: JSON.stringify(req) }
  try {
    return await api(path, init)
  } catch (error) {
    // A lost response is retried once with the exact signed nonce and bytes.
    // HTTP refusals are deliberate and are never automatically retried.
    if (!(error instanceof TypeError)) throw error
    return api(path, init)
  }
}

export async function getBalance(
  address: string,
  chain: VotingChain = inferVotingChain(address),
): Promise<BalanceResponse> {
  const q = new URLSearchParams({ chain })
  return api(`/v1/balances/${encodeURIComponent(address)}?${q}`)
}

export type VotingMetadata = Schemas['Metadata']
export function getMetadata(): Promise<VotingMetadata> {
  return api('/v1/metadata')
}
export type CanonicalResponse = Schemas['CanonicalResponse']
export async function canonicalizeForSigning(
  request:
    | { kind: 'proposal'; sections: ProposalSections }
    | { kind: 'analysis'; sections: AnalysisSections }
    | { kind: 'comment'; body_html: string },
): Promise<CanonicalResponse> {
  const result = await api<CanonicalResponse>('/v1/canonicalize', {
    method: 'POST',
    body: JSON.stringify(request),
  })
  if (result.protocol_version !== 2)
    throw new Error('Unsupported signing protocol; refresh before signing.')
  const bytes = result.canonical_json ?? result.body_html

  if ((await sha256Hex(bytes)) !== result.body_hash)
    throw new Error('Canonical response hash mismatch; signing cancelled.')
  return result
}

export function getFrozenWeight(
  id: string,
  address: string,
  chain: VotingChain,
): Promise<FrozenWeightResponse> {
  return api(
    `/v1/proposals/${encodeURIComponent(id)}/weights/${encodeURIComponent(address)}?${new URLSearchParams({ chain })}`,
  )
}
