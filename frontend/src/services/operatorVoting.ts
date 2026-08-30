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
  as_of_height?: number
  balance: string
  /** Absent on a pre-OV-B6 image — treat as unknown, never as a live 0. */
  registered?: boolean
  pending?: boolean
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
  }
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
