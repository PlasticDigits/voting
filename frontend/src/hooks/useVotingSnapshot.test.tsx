import { act, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { getBalance } from '@/services/operatorVoting'
import { useVotingSnapshot } from './useVotingSnapshot'
vi.mock('@/services/operatorVoting', () => ({
  REGISTER_POLL_TIMEOUT_MS: 120_000,
  getBalance: vi.fn(),
  getRegistration: async () => ({
    bsc: {
      chain: 'bsc',
      wallet_address: '0x1111111111111111111111111111111111111111',
      registered_at_height: 100,
      initial_balance: '1000000000000000000000',
      status: 'active',
    },
  }),
  chainRegistration: (lookup: Record<string, unknown>, chain: string) => lookup?.[chain] ?? null,
  isRegistrationPending: () => false,
  isPollAborted: () => false,
  registerWallet: vi.fn(),
  waitForLedgerRegistration: vi.fn(),
}))
afterEach(() => {
  vi.useRealTimers()
})
describe('registration certification progress', () => {
  it('keeps the balance visible while future eligibility progresses after confirmation', async () => {
    vi.useFakeTimers()
    const balance = {
      chain: 'bsc' as const,
      address: '0x1111111111111111111111111111111111111111',
      height: 100,
      as_of_height: 100,
      balance: '1000000000000000000000',
      registered: true,
      pending: false,
      initial_balance: '1000000000000000000000',
    }
    vi.mocked(getBalance)
      .mockResolvedValueOnce({ ...balance, snapshot_eligible: false })
      .mockResolvedValue({ ...balance, snapshot_eligible: true })
    const { result, unmount } = renderHook(() => useVotingSnapshot(balance.address, 'bsc'))
    await act(async () => {})
    expect(result.current.status).toBe('registered')
    expect(result.current.balance).toBe(balance.balance)
    expect(result.current.snapshotEligible).toBe(false)
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4_000)
    })
    expect(result.current.snapshotEligible).toBe(true)
    expect(result.current.balance).toBe(balance.balance)
    unmount()
  })
})

it('ignores an older account response after switching wallets', async () => {
  let finishOld: (value: Awaited<ReturnType<typeof getBalance>>) => void = () => {}
  const oldResponse = new Promise<Awaited<ReturnType<typeof getBalance>>>((resolve) => {
    finishOld = resolve
  })
  const oldAddress = '0x1111111111111111111111111111111111111111'
  const nextAddress = '0x2222222222222222222222222222222222222222'
  const balance = {
    chain: 'bsc' as const,
    address: nextAddress,
    height: 100,
    as_of_height: 100,
    balance: '0',
    registered: true,
    snapshot_eligible: true,
    pending: false,
    initial_balance: '0',
  }
  vi.mocked(getBalance).mockReset().mockReturnValueOnce(oldResponse).mockResolvedValue(balance)
  const { result, rerender, unmount } = renderHook(
    ({ address }) => useVotingSnapshot(address, 'bsc'),
    { initialProps: { address: oldAddress } },
  )
  await act(async () => {})
  rerender({ address: nextAddress })
  await act(async () => {})
  expect(result.current.balance).toBe('0')
  await act(async () => {
    finishOld({ ...balance, address: oldAddress, balance: '1000000000000000000000' })
  })
  expect(result.current.balance).toBe('0')
  expect(result.current.snapshotEligible).toBe(true)
  unmount()
})
