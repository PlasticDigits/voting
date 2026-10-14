import { describe, expect, it } from 'vitest'
import {
  visibleLen,
  visibleText,
  validateSections,
  GOLDEN_SECTIONS,
  MAX_BODY_BYTES,
} from '@/utils/proposalSections'
import { productionEnvViolations } from '@/utils/prodEnvGuards'
import { buildSignedPayload, canonicalizePayload } from '@/utils/votingPayload'

describe('protocol boundaries', () => {
  it('handles invalid numeric entities without exceptions or invalid scalar replacement', () => {
    for (const entity of ['&#1114112;', '&#x110000;', '&#55296;', '&#xD800;']) {
      expect(() => visibleText(entity)).not.toThrow()
      expect(visibleText(entity)).toBe(entity)
    }
    expect(visibleText('&amp;lt; &AMP; &#xD800;')).toBe('&lt; &AMP; &#xD800;')
    expect(visibleLen('🚀e\u0301&#x1F680;')).toBe(4)
  })
  it('caps UTF8 bytes independently of JavaScript code units', () => {
    const input = { ...GOLDEN_SECTIONS, context: '🚀'.repeat(MAX_BODY_BYTES / 4) }
    expect(input.context.length).toBeLessThan(MAX_BODY_BYTES)
    expect(validateSections(input).ok).toBe(false)
  })
  it('signs a fresh nonce and binds full revision content for open', () => {
    const payload = buildSignedPayload({
      chain: 'bsc',
      address: '0x1111111111111111111111111111111111111111',
      purpose: 'open_vote',
      title: 'Reviewed title',
      body_hash: 'a'.repeat(64),
      revision: 4,
    })
    expect(payload.version).toBe(2)
    expect(payload.nonce).toMatch(/^[\da-f-]{36}$/)
    expect(JSON.parse(canonicalizePayload(payload))).toMatchObject({
      title: 'Reviewed title',
      revision: 4,
      body_hash: 'a'.repeat(64),
    })
    expect(
      buildSignedPayload({ chain: 'bsc', address: payload.address, purpose: 'open_vote' }).nonce,
    ).not.toBe(payload.nonce)
  })
  it('rejects unsafe URLs, local networks and simulated wallets in every build', () => {
    const clean = {
      VITE_OPERATOR_VOTING_URL: 'https://operator.vote.cl8y.com',
      VITE_WC_PROJECT_ID: 'test-build-id',
    }
    for (const url of [
      '',
      'http://api.example',
      'https://user:pass@api.example',
      'https://api.example/?key=secret',
      'https://localhost',
    ]) {
      expect(
        productionEnvViolations({ ...clean, VITE_OPERATOR_VOTING_URL: url }).length,
      ).toBeGreaterThan(0)
    }
    for (const settings of [
      { VITE_DEV_MODE: 'true' },
      { VITE_SIMULATED_WALLET: 'true' },
      { VITE_NETWORK: 'local' },
      { VITE_EVM_CHAIN_ID: '31337' },
      { VITE_TERRA_CHAIN_ID: 'localterra' },
    ]) {
      expect(productionEnvViolations({ ...clean, ...settings }).length).toBeGreaterThan(0)
    }
  })
})
