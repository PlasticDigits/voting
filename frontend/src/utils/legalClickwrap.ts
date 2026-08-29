/**
 * CL8Y Legal clickwrap wiring for voting.
 *
 * Signing happens on the hosted portal; this app only checks status and redirects.
 * Do not reimplement Terra ADR-36 or EVM verify for terms.
 */
import {
  createClient,
  isAllowedRedirectUri,
  sanitizeRedirectUri,
  type ClickwrapClient,
} from '@plasticdigits/cl8y-clickwrap'

export const DEFAULT_LEGAL_PROPERTY = 'vote.cl8y.com'
export const DEFAULT_LEGAL_API_BASE_URL = 'https://api.terms.cl8y.com'
export const DEFAULT_LEGAL_TERMS_BASE_URL = 'https://terms.cl8y.com'

/** Origins the voting dApp may pass as portal `redirect_uri` (portal still enforces its allowlist). */
export const LEGAL_REDIRECT_ALLOWLIST = ['https://vote.cl8y.com'] as const

let clientSingleton: ClickwrapClient | null = null

export function getLegalProperty(): string {
  const fromEnv = import.meta.env.VITE_LEGAL_PROPERTY?.trim()
  return fromEnv || DEFAULT_LEGAL_PROPERTY
}

export function getLegalApiBaseUrl(): string {
  const fromEnv = import.meta.env.VITE_LEGAL_API_BASE_URL?.trim()
  return (fromEnv || DEFAULT_LEGAL_API_BASE_URL).replace(/\/$/, '')
}

export function getLegalTermsBaseUrl(): string {
  const fromEnv = import.meta.env.VITE_LEGAL_TERMS_BASE_URL?.trim()
  return (fromEnv || DEFAULT_LEGAL_TERMS_BASE_URL).replace(/\/$/, '')
}

export function getLegalClickwrapClient(): ClickwrapClient {
  if (!clientSingleton) {
    clientSingleton = createClient({
      apiBaseUrl: getLegalApiBaseUrl(),
      termsBaseUrl: getLegalTermsBaseUrl(),
    })
  }
  return clientSingleton
}

export function resetLegalClickwrapClientForTests(): void {
  clientSingleton = null
}

/**
 * Playwright webServer sets `VITE_PLAYWRIGHT_E2E=true` so E2E is not blocked by Legal.
 * Never enable on production / Coolify builds.
 */
export function skipLegalClickwrapForAutomation(): boolean {
  return import.meta.env.VITE_PLAYWRIGHT_E2E === 'true'
}

export function allowLegalLocalhostRedirect(): boolean {
  return !import.meta.env.PROD
}

export function getLegalRedirectAllowlist(): string[] {
  const extra = import.meta.env.VITE_LEGAL_REDIRECT_ALLOWLIST?.trim()
  if (!extra) return [...LEGAL_REDIRECT_ALLOWLIST]
  const parsed = extra
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
  return [...new Set([...LEGAL_REDIRECT_ALLOWLIST, ...parsed])]
}

function localOriginAllowed(uri: string): boolean {
  try {
    const parsed = new URL(uri)
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false
    if (getLegalRedirectAllowlist().includes(parsed.origin)) return true
    if (allowLegalLocalhostRedirect() && (parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1')) {
      return true
    }
    return false
  } catch {
    return false
  }
}

/**
 * Fail-fast sanitize of `window.location.href` before portal redirect.
 * Path-preserving: return the current URL (list `/`, `/new`, `/:id`, or
 * `/vote*` aliases) so Accept lands on the same page. Origin allowlist only
 * (`https://vote.cl8y.com`); extra path/query/fragment is OK. Portal allowlist
 * remains the source of truth. Do not mint a redirect_uri from Host headers.
 */
export function resolveLegalRedirectUri(href?: string): string | null {
  const candidate = href ?? (typeof window !== 'undefined' ? window.location.href : '')
  if (!candidate || !localOriginAllowed(candidate)) return null
  const options = {
    allowlist: getLegalRedirectAllowlist(),
    allowLocalhost: allowLegalLocalhostRedirect(),
  }
  try {
    return sanitizeRedirectUri(candidate, options) ?? candidate
  } catch {
    return candidate
  }
}

export function isLegalRedirectUriAllowed(uri: string): boolean {
  if (!localOriginAllowed(uri)) return false
  try {
    return isAllowedRedirectUri(uri, {
      allowlist: getLegalRedirectAllowlist(),
      allowLocalhost: allowLegalLocalhostRedirect(),
    })
  } catch {
    return true
  }
}
