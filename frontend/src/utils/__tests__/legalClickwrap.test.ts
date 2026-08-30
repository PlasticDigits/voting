import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DEFAULT_LEGAL_PROPERTY,
  LEGAL_APP_NAME,
  LEGAL_REDIRECT_ALLOWLIST,
  getLegalProperty,
  isLegalRedirectUriAllowed,
  resetLegalClickwrapClientForTests,
  resolveLegalRedirectUri,
  skipLegalClickwrapForAutomation,
} from '@/utils/legalClickwrap'

describe('legalClickwrap', () => {
  beforeEach(() => {
    resetLegalClickwrapClientForTests()
  })

  afterEach(() => {
    vi.unstubAllEnvs()
    resetLegalClickwrapClientForTests()
  })

  it('defaults property to vote.cl8y.com (no property confusion)', () => {
    expect(getLegalProperty()).toBe(DEFAULT_LEGAL_PROPERTY)
    expect(DEFAULT_LEGAL_PROPERTY).toBe('vote.cl8y.com')
    expect(LEGAL_APP_NAME).toBe('CL8Y Voting')
  })

  it('allowlists vote.cl8y.com redirect origins and rejects attacker origins', () => {
    expect(LEGAL_REDIRECT_ALLOWLIST).toContain('https://vote.cl8y.com')
    expect(isLegalRedirectUriAllowed('https://vote.cl8y.com/')).toBe(true)
    expect(isLegalRedirectUriAllowed('https://vote.cl8y.com/vote')).toBe(true)
    expect(isLegalRedirectUriAllowed('https://vote.cl8y.com/new')).toBe(true)
    expect(isLegalRedirectUriAllowed('https://vote.cl8y.com/vote/new')).toBe(true)
    expect(isLegalRedirectUriAllowed('https://evil.example/phish')).toBe(false)
    expect(isLegalRedirectUriAllowed('javascript:alert(1)')).toBe(false)
    expect(isLegalRedirectUriAllowed('data:text/html,phish')).toBe(false)
    expect(isLegalRedirectUriAllowed('//evil.example')).toBe(false)
    expect(isLegalRedirectUriAllowed('https://vote.cl8y.com.evil/vote')).toBe(false)
    expect(isLegalRedirectUriAllowed('https://vote.cl8y.com@evil.example/')).toBe(false)
    expect(isLegalRedirectUriAllowed('http://vote.cl8y.com/')).toBe(false)
  })

  it('sanitizes window location for portal redirect_uri (path-preserving)', () => {
    window.history.replaceState({}, '', '/vote')
    const uri = resolveLegalRedirectUri()
    expect(uri).toBeTruthy()
    expect(uri).toMatch(/^http:\/\/localhost/)
    expect(uri).toMatch(/\/vote$/)

    const root = resolveLegalRedirectUri('http://localhost:5176/')
    expect(root).toBe('http://localhost:5176/')

    const nested = resolveLegalRedirectUri('http://localhost:5176/new')
    expect(nested).toMatch(/\/new$/)
  })

  it('skips gate only when VITE_PLAYWRIGHT_E2E is true', () => {
    vi.stubEnv('VITE_PLAYWRIGHT_E2E', 'true')
    expect(skipLegalClickwrapForAutomation()).toBe(true)
    vi.stubEnv('VITE_PLAYWRIGHT_E2E', 'false')
    expect(skipLegalClickwrapForAutomation()).toBe(false)
  })
})
