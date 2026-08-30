import { describe, expect, it } from 'vitest'
import {
  isPollAborted,
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
    let t = 0
    await expect(
      waitForLedgerRegistration('terra1x', 'terra', {
        timeoutMs: 2,
        intervalMs: 1,
        now: () => t,
        sleep: async (ms) => {
          t += ms
        },
        lookup: async () => ({ terra: null, pending: { terra: true } }),
      })
    ).rejects.toThrow(/still pending/)
  })

  it('backs off the poll interval up to the cap', async () => {
    const pauses: number[] = []
    let t = 0
    await waitForLedgerRegistration('terra1x', 'terra', {
      timeoutMs: 20,
      intervalMs: 2,
      maxIntervalMs: 8,
      now: () => t,
      sleep: async (ms) => {
        pauses.push(ms)
        t += ms
      },
      lookup: async () => ({ terra: null, pending: { terra: true } }),
    }).catch(() => undefined)
    expect(pauses[0]).toBe(2)
    expect(pauses[1]).toBe(4)
    expect(pauses[2]).toBe(8)
    expect(pauses.slice(3).every((ms) => ms <= 8)).toBe(true)
  })

  it('aborts without claiming the snapshot is pending', async () => {
    const ac = new AbortController()
    ac.abort()
    await expect(
      waitForLedgerRegistration('terra1x', 'terra', {
        timeoutMs: 5_000,
        intervalMs: 1,
        signal: ac.signal,
        lookup: async () => ({ terra: null, pending: { terra: true } }),
      })
    ).rejects.toSatisfy((err: unknown) => isPollAborted(err))
  })
})
