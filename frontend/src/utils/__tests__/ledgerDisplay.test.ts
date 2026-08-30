import { describe, expect, it } from 'vitest'
import { snapshotBalanceRaw } from '@/utils/ledgerDisplay'

describe('ledger display', () => {
  it('hides amounts until the wallet is ledger-registered', () => {
    expect(
      snapshotBalanceRaw({
        chain: 'terra',
        address: 'terra1x',
        height: 0,
        balance: '0',
        registered: false,
        pending: false,
      })
    ).toBeNull()
    expect(
      snapshotBalanceRaw({
        chain: 'terra',
        address: 'terra1x',
        height: 0,
        balance: '0',
        registered: false,
        pending: true,
      })
    ).toBeNull()
  })

  it('shows a registered true-zero and a registered holding', () => {
    expect(
      snapshotBalanceRaw({
        chain: 'terra',
        address: 'terra1x',
        height: 10,
        balance: '0',
        registered: true,
        pending: false,
      })
    ).toBe('0')
    expect(
      snapshotBalanceRaw({
        chain: 'bsc',
        address: '0xabc',
        height: 80,
        as_of_height: 80,
        balance: '1000000000000000000000',
        registered: true,
        pending: false,
      })
    ).toBe('1000000000000000000000')
  })

  it('treats a missing registered flag as unknown, not a live 0 (OV-B6)', () => {
    expect(
      snapshotBalanceRaw({
        chain: 'terra',
        address: 'terra1x',
        height: 30162104,
        balance: '0',
      })
    ).toBeNull()
  })
})
