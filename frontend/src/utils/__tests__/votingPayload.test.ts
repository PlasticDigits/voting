import { describe, expect, it } from 'vitest'
import {
  assertPurposeSeparation,
  buildSignedPayload,
  canPropose,
  canonicalizePayload,
} from '@/utils/votingPayload'
import { MIN_PROPOSAL_RAW, VOTING_APP } from '@/utils/constants'

describe('voting payload', () => {
  it('canonicalizes with serde field order and omits empty optionals', () => {
    const payload = buildSignedPayload({
      chain: 'terra',
      purpose: 'register',
      address: 'terra1abc',
      now: 1_700_000_000,
    })
    expect(canonicalizePayload(payload)).toBe(
      '{"app":"cl8y-voting","chain":"terra","chain_id":"columbus-5","purpose":"register","address":"terra1abc","issued_at":1700000000,"expires_at":1700000600}'
    )
  })

  it('includes propose fields in order after the base keys', () => {
    const payload = buildSignedPayload({
      chain: 'bsc',
      purpose: 'propose',
      address: '0xabc',
      now: 1_700_000_000,
      title: 'Hello',
      body_hash: 'deadbeef',
    })
    expect(canonicalizePayload(payload)).toBe(
      '{"app":"cl8y-voting","chain":"bsc","chain_id":"56","purpose":"propose","address":"0xabc","issued_at":1700000000,"expires_at":1700000600,"title":"Hello","body_hash":"deadbeef"}'
    )
  })

  it('domain-separates purposes', () => {
    const payload = buildSignedPayload({
      chain: 'terra',
      purpose: 'register',
      address: 'terra1abc',
      now: 1,
    })
    expect(payload.app).toBe(VOTING_APP)
    expect(() => assertPurposeSeparation(payload, 'vote')).toThrow(/cannot be used as vote/)
    expect(() => assertPurposeSeparation(payload, 'register')).not.toThrow()
  })

  it('propose gate is disabled below 1000 CL8Y and enabled at the boundary', () => {
    expect(canPropose(MIN_PROPOSAL_RAW - 1n)).toBe(false)
    expect(canPropose(MIN_PROPOSAL_RAW)).toBe(true)
    expect(canPropose(MIN_PROPOSAL_RAW + 1n)).toBe(true)
    expect(canPropose('0')).toBe(false)
  })
})
