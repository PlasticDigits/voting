import { useCallback, useEffect, useRef, useState } from 'react'
import {
  chainRegistration,
  getBalance,
  getRegistration,
  isPollAborted,
  isRegistrationPending,
  registerWallet,
  waitForLedgerRegistration,
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
  error: string | null
  loading: boolean
  polling: boolean
  lastPollAt: number | null
  canRetry: boolean
  refresh: () => Promise<void>
  retryPending: () => Promise<void>
  completeRegister: (signed: SignedRequest) => Promise<void>
}

export function useVotingSnapshot(address: string | null, chain: VotingChain | null): VotingSnapshot {
  const [status, setStatus] = useState<SnapshotStatus>('idle')
  const [balance, setBalance] = useState<string | null>(null)
  const [asOfHeight, setAsOfHeight] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [polling, setPolling] = useState(false)
  const [lastPollAt, setLastPollAt] = useState<number | null>(null)
  const abortRef = useRef<AbortController | null>(null)
  const resumeOnPending = useRef(true)

  const applyBalance = useCallback((bal: BalanceResponse, registered: boolean, pending: boolean) => {
    if (registered) {
      setStatus('registered')
      setBalance(snapshotBalanceRaw({ ...bal, registered: true }))
      setAsOfHeight(bal.as_of_height ?? bal.height)
      return
    }
    setBalance(null)
    setAsOfHeight(null)
    setStatus(pending ? 'pending' : 'unregistered')
  }, [])

  const stopPoll = useCallback(() => {
    abortRef.current?.abort()
    abortRef.current = null
    setPolling(false)
  }, [])

  const refresh = useCallback(async () => {
    if (!address || !chain) {
      stopPoll()
      setStatus('idle')
      setBalance(null)
      setAsOfHeight(null)
      setError(null)
      setLastPollAt(null)
      return
    }
    setLoading(true)
    try {
      const [reg, bal] = await Promise.all([getRegistration(address), getBalance(address, chain)])
      const registered = Boolean(chainRegistration(reg, chain) || bal.registered === true)
      const pending = !registered && (isRegistrationPending(reg, chain) || bal.pending === true)
      applyBalance(bal, registered, pending)
      setError(null)
    } catch (err) {
      setStatus('unregistered')
      setBalance(null)
      setAsOfHeight(null)
      setError(err instanceof Error ? err.message : 'Failed to load registration')
    } finally {
      setLoading(false)
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

  const retryPending = useCallback(async () => {
    resumeOnPending.current = false
    await waitUntilRegistered()
  }, [waitUntilRegistered])

  const completeRegister = useCallback(
    async (signed: SignedRequest) => {
      if (!address || !chain) return
      setError(null)
      const res = await registerWallet(signed)
      const ledgerRow = Boolean(res.registration) && res.pending === false
      if (ledgerRow) {
        await refresh()
        return
      }
      setStatus('pending')
      setBalance(null)
      resumeOnPending.current = false
      await waitUntilRegistered()
    },
    [address, chain, refresh, waitUntilRegistered]
  )

  const canRetry = status === 'pending' && !polling && Boolean(error)

  return {
    status,
    balance,
    asOfHeight,
    error,
    loading,
    polling,
    lastPollAt,
    canRetry,
    refresh,
    retryPending,
    completeRegister,
  }
}
