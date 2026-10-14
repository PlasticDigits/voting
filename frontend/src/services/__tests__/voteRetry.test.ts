import { afterEach, describe, expect, it, vi } from 'vitest'
import { castVote } from '@/services/operatorVoting'
afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})
describe('response-loss vote retry', () => {
  it('retries exact signed bytes and nonce once without obtaining another signature', async () => {
    vi.stubEnv('VITE_OPERATOR_VOTING_URL', 'https://operator.example')
    const result = {
      ok: true,
      weight: '1000000000000000000000',
      receipt_id: 'receipt',
      snapshot: { terra_height: 100, bsc_block: 100 },
    }
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new TypeError('connection lost after commit'))
      .mockResolvedValue(new Response(JSON.stringify(result), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    const request = {
      chain: 'bsc' as const,
      address: '0x1111111111111111111111111111111111111111',
      payload: '{"version":2,"nonce":"fixed-accepted-action"}',
      signature: 'synthetic',
      choice: 'for' as const,
    }
    expect(await castVote('poll', request)).toEqual(result)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(fetchMock.mock.calls[0][1].body).toBe(fetchMock.mock.calls[1][1].body)
    expect(JSON.parse(fetchMock.mock.calls[1][1].body).payload).toBe(request.payload)
  })
  it('does not retry a deliberate HTTP conflict', async () => {
    vi.stubEnv('VITE_OPERATOR_VOTING_URL', 'https://operator.example')
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ error: 'vote already recorded', code: 'conflict' }), {
          status: 409,
        }),
      )
    vi.stubGlobal('fetch', fetchMock)
    await expect(
      castVote('poll', {
        chain: 'bsc',
        address: '0x1111111111111111111111111111111111111111',
        payload: 'synthetic',
        signature: 'synthetic',
        choice: 'for',
      }),
    ).rejects.toThrow('vote already recorded')
    expect(fetchMock).toHaveBeenCalledOnce()
  })
})
