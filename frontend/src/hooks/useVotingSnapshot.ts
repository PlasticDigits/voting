import { useCallback, useEffect, useState } from 'react'
import {
  chainRegistration,
  getBalance,
  getRegistration,
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
  refresh: () => Promise<void>
  completeRegister: (signed: SignedRequest) => Promise<void>
}

export function useVotingSnapshot(address: string | null, chain: VotingChain | null): VotingSnapshot {
  const [status, setStatus] = useState<SnapshotStatus>('idle')
  const [balance, setBalance] = useState<string | null>(null)
  const [asOfHeight, setAsOfHeight] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

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

  const refresh = useCallback(async () => {
    if (!address || !chain) {
      setStatus('idle')
      setBalance(null)
      setAsOfHeight(null)
      setError(null)
      return
    }
    setLoading(true)
    try {
      const [reg, bal] = await Promise.all([getRegistration(address), getBalance(address, chain)])
      const registered = Boolean(chainRegistration(reg, chain) || bal.registered)
      const pending = !registered && (isRegistrationPending(reg, chain) || bal.pending)
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
  }, [address, chain, applyBalance])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const completeRegister = useCallback(
    async (signed: SignedRequest) => {
      if (!address || !chain) return
      setError(null)
      const res = await registerWallet(signed)
      const ledgerRow = Boolean(res.registration) && res.pending === false
      if (!ledgerRow) {
        setStatus('pending')
        setBalance(null)
        await waitForLedgerRegistration(address, chain)
      }
      await refresh()
    },
    [address, chain, refresh]
  )

  return { status, balance, asOfHeight, error, loading, refresh, completeRegister }
}
