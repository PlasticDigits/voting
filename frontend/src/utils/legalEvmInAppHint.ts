/**
 * EVM Legal next-step after connect (issue #16).
 *
 * Voting WalletConnect / injected sessions do not follow the user to
 * `terms.cl8y.com`. Chrome/Safari then hit Legal “No EVM wallet found”.
 * Surface Open-in-MetaMask + copy instead of a dead Accept. Do **not**
 * implement portal `personal_sign` here (C1 / #5).
 *
 * Binance Web3 has no first-party documented dapp-browser URL we can
 * ship; copy-link + MetaMask (documented `link.metamask.io/dapp/`) is
 * the path. Do not invent `bnc://` schemes.
 */
import {
  getLegalProperty,
  getLegalTermsBaseUrl,
  LEGAL_APP_NAME,
} from '@/utils/legalClickwrap'

export const METAMASK_DAPP_BROWSER_ORIGIN = 'https://link.metamask.io'

export const LEGAL_EVM_INAPP_HINT =
  'Chrome and Safari cannot finish terms. Open the Legal page in MetaMask, or paste the link in Binance Web3.'

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
 * Show when unsigned **and** this page cannot finish terms in-place:
 * no EIP-1193 inject, or the session is WalletConnect (Legal will not see it).
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
