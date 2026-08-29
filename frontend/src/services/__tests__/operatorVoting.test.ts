import { describe, expect, it } from 'vitest'
import {
  waitForLedgerRegistration,
  type RegistrationLookup,
} from '@/services/operatorVoting'

describe('waitForLedgerRegistration', () => {
  it('returns once the ledger row exists', async () => {
    let n = 0
    const lookup = async (): Promise<RegistrationLookup | null> => {
      n += 1
      if (n < 3) {
        return { terra: null, bsc: null, pending: { terra: true, bsc: false } }
      }
      return {
        terra: { chain: 'terra', wallet_address: 'terra1x', registered_at_height: 10, status: 'active' },
        bsc: null,
        pending: { terra: false, bsc: false },
      }
    }
    let t = 0
    const found = await waitForLedgerRegistration('terra1x', 'terra', {
      timeoutMs: 5_000,
      intervalMs: 1,
      now: () => t,
      sleep: async (ms) => {
        t += ms
      },
      lookup,
    })
    expect(found.terra?.status).toBe('active')
    expect(n).toBe(3)
  })

  it('times out if the snapshot never lands', async () => {
    await expect(
      waitForLedgerRegistration('terra1x', 'terra', {
        timeoutMs: 2,
        intervalMs: 1,
        now: (() => {
          let t = 0
          return () => {
            t += 1
            return t
          }
        })(),
        sleep: async () => undefined,
        lookup: async () => ({ terra: null, pending: { terra: true } }),
      })
    ).rejects.toThrow(/still pending/)
  })
})
