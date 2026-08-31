/**
 * EVM Legal shortcut after connect (issue #16).
 *
 * Accept (`@plasticdigits/cl8y-clickwrap` >= 0.1.1) full-navigates to the
 * portal with `account` on the query. The portal (Legal #15, closed) has
 * Open in MetaMask / Binance Web3 / Copy / WalletConnect — Chrome/Safari
 * can finish terms there. This hint is a same-page shortcut, not a claim
 * that Accept is a dead end. Do **not** implement portal `personal_sign`
 * here (C1 / #5).
 *
 * Voting does not ship a Binance dapp-browser URL; Copy link + MetaMask
 * (`link.metamask.io/dapp/`) is the path. Do not invent `bnc://`.
 */
import {
  getLegalProperty,
  getLegalTermsBaseUrl,
  LEGAL_APP_NAME,
} from '@/utils/legalClickwrap'

export const METAMASK_DAPP_BROWSER_ORIGIN = 'https://link.metamask.io'

export const LEGAL_EVM_INAPP_HINT =
  'Accept opens the Legal page. On a phone, open it in MetaMask or paste the link in Binance Web3.'

export function hasInjectedEip1193(
  win: { ethereum?: unknown } = typeof window !== 'undefined' ? window : {}
): boolean {
  return Boolean(win.ethereum)
}

export function isEvmWalletConnectConnectorId(connectorId: string | null | undefined): boolean {
  if (!connectorId) return false
  return connectorId === 'walletConnect' || connectorId.startsWith('walletConnect')
}

/**
 * Show when unsigned **and** this tab cannot sign in-place: no EIP-1193
 * inject, or the session is WalletConnect (the portal will not see it).
 */
export function shouldShowLegalEvmInAppHint(input: {
  hasInjectedEip1193: boolean
  connectedViaWalletConnect: boolean
  signedLatest: boolean | null
}): boolean {
  if (input.signedLatest !== false) return false
  if (input.connectedViaWalletConnect) return true
  return !input.hasInjectedEip1193
}

export function isAllowedLegalEvmSignUrl(
  href: string,
  termsBaseUrl: string = getLegalTermsBaseUrl()
): boolean {
  try {
    const url = new URL(href)
    const terms = new URL(termsBaseUrl)
    if (url.origin !== terms.origin) return false
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return false
    if (url.pathname !== '/sign/evm') return false
    return true
  } catch {
    return false
  }
}

/**
 * Build the portal EVM sign URL from the terms origin + connected store
 * address. Never read `account` from the page query. `account` is not a
 * redirect target.
 */
export function buildLegalEvmSignUrl(input: {
  account: string
  redirectUri?: string | null
  property?: string
  termsBaseUrl?: string
}): string | null {
  const account = input.account.trim()
  if (!/^0x[0-9a-fA-F]{40}$/.test(account)) return null
  const termsBase = (input.termsBaseUrl ?? getLegalTermsBaseUrl()).replace(/\/$/, '')
  const property = input.property ?? getLegalProperty()
  let parsed: URL
  try {
    parsed = new URL(`${termsBase}/sign/evm`)
  } catch {
    return null
  }
  parsed.searchParams.set('property', property)
  if (input.redirectUri) parsed.searchParams.set('redirect_uri', input.redirectUri)
  parsed.searchParams.set('app_name', LEGAL_APP_NAME)
  parsed.searchParams.set('account', account)
  const out = parsed.toString()
  return isAllowedLegalEvmSignUrl(out, termsBase) ? out : null
}

export function isAllowedMetaMaskDappLink(href: string): boolean {
  try {
    const url = new URL(href)
    if (url.origin !== METAMASK_DAPP_BROWSER_ORIGIN) return false
    if (url.protocol !== 'https:') return false
    return url.pathname.startsWith('/dapp/')
  } catch {
    return false
  }
}

/**
 * Documented MetaMask in-app browser deeplink:
 * `https://link.metamask.io/dapp/{host}{path}{search}`
 * (https://docs.metamask.io/metamask-connect/evm/guides/metamask-exclusive/use-deeplinks/).
 * Wraps only an already-allowlisted Legal `/sign/evm` URL.
 */
export function buildMetaMaskDappBrowserUrl(legalSignUrl: string): string | null {
  if (!isAllowedLegalEvmSignUrl(legalSignUrl)) return null
  try {
    const parsed = new URL(legalSignUrl)
    const dapp = `${parsed.host}${parsed.pathname}${parsed.search}`
    const href = `${METAMASK_DAPP_BROWSER_ORIGIN}/dapp/${dapp}`
    return isAllowedMetaMaskDappLink(href) ? href : null
  } catch {
    return null
  }
}
