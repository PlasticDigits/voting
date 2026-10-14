import { useCallback, useEffect, useRef, useState } from 'react'
import {
  chainRegistration,
  getBalance,
  getRegistration,
  isPollAborted,
  isRegistrationPending,
  registerWallet,
  waitForLedgerRegistration,
  REGISTER_POLL_TIMEOUT_MS,
  type BalanceResponse,
  type SignedRequest,
} from '@/services/operatorVoting'
import { snapshotBalanceRaw } from '@/utils/ledgerDisplay'
import type { VotingChain } from '@/utils/votingPayload'

export type SnapshotStatus = 'idle' | 'unregistered' | 'pending' | 'registered'

export type VotingSnapshot = {
  status: SnapshotStatus
  balance: string | null
  asOfHeight: number | null
  snapshotEligible: boolean | null
  error: string | null
  loading: boolean
  polling: boolean
  lastPollAt: number | null
  canRetry: boolean
  refresh: () => Promise<void>
  retryPending: () => Promise<void>
  completeRegister: (signed: SignedRequest) => Promise<void>
}

export function useVotingSnapshot(
  address: string | null,
  chain: VotingChain | null,
): VotingSnapshot {
  const identityKey = address && chain ? `${chain}:${address.trim().toLowerCase()}` : null
  const currentIdentity = useRef(identityKey)
  currentIdentity.current = identityKey
  const [dataIdentity, setDataIdentity] = useState<string | null>(null)
  const [status, setStatus] = useState<SnapshotStatus>('idle')
  const [balance, setBalance] = useState<string | null>(null)
  const [snapshotEligible, setSnapshotEligible] = useState<boolean | null>(null)
  const [asOfHeight, setAsOfHeight] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [polling, setPolling] = useState(false)
  const [lastPollAt, setLastPollAt] = useState<number | null>(null)
  const abortRef = useRef<AbortController | null>(null)
  const resumeOnPending = useRef(true)

  const applyBalance = useCallback(
    (bal: BalanceResponse, registered: boolean, pending: boolean) => {
      if (registered) {
        setStatus('registered')
        setSnapshotEligible(bal.snapshot_eligible === true)
        setBalance(snapshotBalanceRaw({ ...bal, registered: true }))
        setAsOfHeight(bal.as_of_height ?? bal.height)
        return
      }
      setSnapshotEligible(null)
      setBalance(null)
      setAsOfHeight(null)
      setStatus(pending ? 'pending' : 'unregistered')
    },
    [],
  )

  const stopPoll = useCallback(() => {
    abortRef.current?.abort()
    abortRef.current = null
    setPolling(false)
  }, [])

  const refresh = useCallback(async () => {
    const requestedIdentity = address && chain ? `${chain}:${address.trim().toLowerCase()}` : null
    if (!address || !chain) {
      stopPoll()
      setStatus('idle')
      setLoading(false)
      setDataIdentity(null)
      setSnapshotEligible(null)
      setBalance(null)
      setAsOfHeight(null)
      setError(null)
      setLastPollAt(null)
      return
    }
    setLoading(true)
    try {
      const [reg, bal] = await Promise.all([getRegistration(address), getBalance(address, chain)])
      if (currentIdentity.current !== requestedIdentity) return
      setDataIdentity(requestedIdentity)
      const registered = Boolean(chainRegistration(reg, chain) || bal.registered === true)
      const pending = !registered && (isRegistrationPending(reg, chain) || bal.pending === true)
      applyBalance(bal, registered, pending)
      setError(null)
    } catch (err) {
      if (currentIdentity.current !== requestedIdentity) return
      setDataIdentity(requestedIdentity)
      setStatus('unregistered')
      setSnapshotEligible(null)
      setBalance(null)
      setAsOfHeight(null)
      setError(err instanceof Error ? err.message : 'Failed to load registration')
    } finally {
      if (currentIdentity.current === requestedIdentity) setLoading(false)
    }
  }, [address, chain, applyBalance, stopPoll])

  const waitUntilRegistered = useCallback(async () => {
    if (!address || !chain) return
    abortRef.current?.abort()
    const ac = new AbortController()
    abortRef.current = ac
    setPolling(true)
    setError(null)
    try {
      await waitForLedgerRegistration(address, chain, {
        signal: ac.signal,
        onAttempt: (at) => setLastPollAt(at),
      })
      if (ac.signal.aborted) return
      await refresh()
    } catch (err) {
      if (ac.signal.aborted || isPollAborted(err)) return
      setError(err instanceof Error ? err.message : 'Registration snapshot is still pending')
    } finally {
      if (abortRef.current === ac) {
        setPolling(false)
        abortRef.current = null
      }
    }
  }, [address, chain, refresh])

  useEffect(() => {
    resumeOnPending.current = true
    setLastPollAt(null)
    void refresh()
    return () => {
      abortRef.current?.abort()
      abortRef.current = null
    }
  }, [refresh])

  useEffect(() => {
    if (status !== 'pending' || !resumeOnPending.current) return
    resumeOnPending.current = false
    void waitUntilRegistered()
  }, [status, waitUntilRegistered])

  // Registration can precede finalized/certified admission. Refresh that state
  // without submitting another intent or changing an existing poll's weight.
  useEffect(() => {
    if (dataIdentity !== identityKey || status !== 'registered' || snapshotEligible !== false)
      return
    let stopped = false
    let timer: ReturnType<typeof setTimeout>
    const deadline = Date.now() + REGISTER_POLL_TIMEOUT_MS
    const poll = async () => {
      if (stopped) return
      await refresh()
      if (!stopped && Date.now() < deadline) timer = setTimeout(() => void poll(), 4_000)
    }
    timer = setTimeout(() => void poll(), 4_000)
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [dataIdentity, identityKey, status, snapshotEligible, refresh])

  const retryPending = useCallback(async () => {
    resumeOnPending.current = false
    await waitUntilRegistered()
  }, [waitUntilRegistered])

  const completeRegister = useCallback(
    async (signed: SignedRequest) => {
      if (!address || !chain) return
      setError(null)
      const requestedIdentity = `${chain}:${address.trim().toLowerCase()}`
      if (currentIdentity.current !== requestedIdentity) return
      const res = await registerWallet(signed)
      if (currentIdentity.current !== requestedIdentity) return
      const ledgerRow = Boolean(res.registration) && res.pending === false
      if (ledgerRow) {
        await refresh()
        return
      }
      setStatus('pending')
      setSnapshotEligible(null)
      setBalance(null)
      resumeOnPending.current = false
      await waitUntilRegistered()
    },
    [address, chain, refresh, waitUntilRegistered],
  )

  const canRetry = status === 'pending' && !polling && Boolean(error)

  return {
    status: dataIdentity === identityKey ? status : 'idle',
    balance: dataIdentity === identityKey ? balance : null,
    asOfHeight: dataIdentity === identityKey ? asOfHeight : null,
    snapshotEligible: dataIdentity === identityKey ? snapshotEligible : null,
    error: dataIdentity === identityKey ? error : null,
    loading,
    polling,
    lastPollAt,
    canRetry,
    refresh,
    retryPending,
    completeRegister,
  }
}
