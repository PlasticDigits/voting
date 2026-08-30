import { describe, expect, it } from 'vitest'
import {
  buildLegalEvmSignUrl,
  buildMetaMaskDappBrowserUrl,
  hasInjectedEip1193,
  isAllowedLegalEvmSignUrl,
  isAllowedMetaMaskDappLink,
  isEvmWalletConnectConnectorId,
  LEGAL_EVM_INAPP_HINT,
  METAMASK_DAPP_BROWSER_ORIGIN,
  shouldShowLegalEvmInAppHint,
} from '../legalEvmInAppHint'
import { LEGAL_APP_NAME } from '../legalClickwrap'

const ACCOUNT = '0x1111111111111111111111111111111111111111'
const TERMS = 'https://terms.cl8y.com'

describe('shouldShowLegalEvmInAppHint (issue #16)', () => {
  it('shows for unsigned EVM with no inject', () => {
    expect(
      shouldShowLegalEvmInAppHint({
        hasInjectedEip1193: false,
        connectedViaWalletConnect: false,
        signedLatest: false,
      })
    ).toBe(true)
  })

  it('shows for WalletConnect even when inject is present (Legal will not see this session)', () => {
    expect(
      shouldShowLegalEvmInAppHint({
        hasInjectedEip1193: true,
        connectedViaWalletConnect: true,
        signedLatest: false,
      })
    ).toBe(true)
  })

  it('hides when already signed, unknown status, or injected without WC', () => {
    expect(
      shouldShowLegalEvmInAppHint({
        hasInjectedEip1193: true,
        connectedViaWalletConnect: false,
        signedLatest: false,
      })
    ).toBe(false)
    expect(
      shouldShowLegalEvmInAppHint({
        hasInjectedEip1193: false,
        connectedViaWalletConnect: false,
        signedLatest: true,
      })
    ).toBe(false)
    expect(
      shouldShowLegalEvmInAppHint({
        hasInjectedEip1193: false,
        connectedViaWalletConnect: false,
        signedLatest: null,
      })
    ).toBe(false)
  })
})

describe('hasInjectedEip1193 / WalletConnect connector id', () => {
  it('detects window.ethereum and walletConnect ids only', () => {
    expect(hasInjectedEip1193({})).toBe(false)
    expect(hasInjectedEip1193({ ethereum: { request: () => undefined } })).toBe(true)
    expect(isEvmWalletConnectConnectorId(null)).toBe(false)
    expect(isEvmWalletConnectConnectorId('mock')).toBe(false)
    expect(isEvmWalletConnectConnectorId('injected')).toBe(false)
    expect(isEvmWalletConnectConnectorId('walletConnect')).toBe(true)
  })
})

describe('buildLegalEvmSignUrl', () => {
  it('includes property, app_name, redirect_uri, and the connected 0x account', () => {
    const url = buildLegalEvmSignUrl({
      account: ACCOUNT,
      redirectUri: 'https://vote.cl8y.com/vote',
      property: 'vote.cl8y.com',
      termsBaseUrl: TERMS,
    })
    expect(url).toBeTruthy()
    const parsed = new URL(url!)
    expect(parsed.origin).toBe(TERMS)
    expect(parsed.pathname).toBe('/sign/evm')
    expect(parsed.searchParams.get('property')).toBe('vote.cl8y.com')
    expect(parsed.searchParams.get('app_name')).toBe(LEGAL_APP_NAME)
    expect(parsed.searchParams.get('redirect_uri')).toBe('https://vote.cl8y.com/vote')
    expect(parsed.searchParams.get('account')).toBe(ACCOUNT)
    expect(isAllowedLegalEvmSignUrl(url!, TERMS)).toBe(true)
  })

  it('rejects non-0x accounts and does not treat account as a redirect', () => {
    expect(
      buildLegalEvmSignUrl({
        account: 'javascript:alert(1)',
        termsBaseUrl: TERMS,
      })
    ).toBeNull()
    expect(
      buildLegalEvmSignUrl({
        account: 'https://evil.example/phish',
        termsBaseUrl: TERMS,
      })
    ).toBeNull()
    expect(
      buildLegalEvmSignUrl({
        account: 'terra1unsignedexample',
        termsBaseUrl: TERMS,
      })
    ).toBeNull()
  })

  it('rejects attacker terms origins', () => {
    expect(isAllowedLegalEvmSignUrl('https://evil.example/sign/evm', TERMS)).toBe(false)
    expect(isAllowedLegalEvmSignUrl('javascript:alert(1)', TERMS)).toBe(false)
    expect(isAllowedLegalEvmSignUrl(`${TERMS}/sign/terra-classic`, TERMS)).toBe(false)
  })
})

describe('buildMetaMaskDappBrowserUrl', () => {
  it('wraps only an allowlisted Legal sign URL', () => {
    const sign = buildLegalEvmSignUrl({
      account: ACCOUNT,
      redirectUri: 'https://vote.cl8y.com/',
      termsBaseUrl: TERMS,
    })
    expect(sign).toBeTruthy()
    const href = buildMetaMaskDappBrowserUrl(sign!)
    expect(href).toBeTruthy()
    expect(href!.startsWith(`${METAMASK_DAPP_BROWSER_ORIGIN}/dapp/`)).toBe(true)
    expect(href).toContain('terms.cl8y.com/sign/evm')
    expect(href).toContain(ACCOUNT)
    expect(isAllowedMetaMaskDappLink(href!)).toBe(true)
  })

  it('does not wrap arbitrary https', () => {
    expect(buildMetaMaskDappBrowserUrl('https://evil.example/phish')).toBeNull()
    expect(isAllowedMetaMaskDappLink('https://evil.example/dapp/x')).toBe(false)
    expect(isAllowedMetaMaskDappLink('javascript:alert(1)')).toBe(false)
  })
})

describe('LEGAL_EVM_INAPP_HINT copy', () => {
  it('is retail-short and does not push a desktop extension or call Legal a vote', () => {
    expect(LEGAL_EVM_INAPP_HINT).toMatch(/MetaMask/)
    expect(LEGAL_EVM_INAPP_HINT).toMatch(/Binance Web3/)
    expect(LEGAL_EVM_INAPP_HINT).not.toMatch(/extension/i)
    expect(LEGAL_EVM_INAPP_HINT).not.toMatch(/install MetaMask/i)
    expect(LEGAL_EVM_INAPP_HINT).not.toMatch(/\bvote\b/i)
  })
})
