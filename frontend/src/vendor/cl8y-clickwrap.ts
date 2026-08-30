/**
 * Local fallback for `@plasticdigits/cl8y-clickwrap`.
 *
 * The published SDK from GitLab project 82547916 installed successfully in this
 * worktree. Keep this file so the app still compiles if registry auth is missing.
 * Production must use the published package, not this fallback.
 */

export type Network = 'EVM' | 'Solana' | 'TerraClassic' | 'Telegram'
export type SignUrlKey = 'telegram' | 'evm' | 'terra_classic' | 'solana'

export interface SignUrls {
  telegram: string
  evm: string
  terra_classic: string
  solana: string
}

export interface TermsLatest {
  property: string
  version_label: string
  effective_date: string
  content_sha256: string
  published_at: string
  sign_urls: SignUrls
}

export interface StatusResponse {
  property: string
  latest_version: string | null
  signed_latest: boolean
  signed_version: string | null
  signed_at: string | null
}

export interface SubmitResponse {
  id: string
  signed_at: string
}

export interface ClientConfig {
  apiBaseUrl?: string
  termsBaseUrl?: string
}

export const DEFAULT_API_BASE_URL = 'https://api.terms.cl8y.com'
export const DEFAULT_TERMS_BASE_URL = 'https://terms.cl8y.com'

export const NETWORK_API_VALUES: Record<Network, string> = {
  EVM: 'EVM',
  Solana: 'SOLANA',
  TerraClassic: 'TERRA_CLASSIC',
  Telegram: 'TELEGRAM',
}

export const NETWORK_SIGN_URL_KEYS: Record<Network, SignUrlKey> = {
  EVM: 'evm',
  Solana: 'solana',
  TerraClassic: 'terra_classic',
  Telegram: 'telegram',
}

export interface ClickwrapClient {
  readonly apiBaseUrl: string
  readonly termsBaseUrl: string
  getTermsLatest(property: string): Promise<TermsLatest>
  getTermsContent(property: string): Promise<string>
  getSignatureStatus(property: string, network: string, account: string): Promise<StatusResponse>
  submitWallet(body: Record<string, unknown>): Promise<SubmitResponse>
  submitTelegram(body: Record<string, unknown>): Promise<SubmitResponse>
}

export function createClient(config: ClientConfig = {}): ClickwrapClient {
  const apiBaseUrl = (config.apiBaseUrl ?? DEFAULT_API_BASE_URL).replace(/\/$/, '')
  const termsBaseUrl = (config.termsBaseUrl ?? DEFAULT_TERMS_BASE_URL).replace(/\/$/, '')

  async function api<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`${apiBaseUrl}${path}`, init)
    if (!res.ok) {
      const body = (await res.json().catch(() => ({}))) as { error?: string }
      throw new Error(body.error ?? res.statusText)
    }
    const contentType = res.headers?.get('content-type') ?? ''
    if (contentType.includes('text/plain')) {
      return (await res.text()) as T
    }
    return res.json() as Promise<T>
  }

  return {
    apiBaseUrl,
    termsBaseUrl,
    getTermsLatest(property: string) {
      return api<TermsLatest>(`/api/v1/terms/latest?property=${encodeURIComponent(property)}`)
    },
    getTermsContent(property: string) {
      return api<string>(`/api/v1/terms/latest/content?property=${encodeURIComponent(property)}`)
    },
    getSignatureStatus(property: string, network: string, account: string) {
      const q = new URLSearchParams({ property, network, account })
      return api<StatusResponse>(`/api/v1/signatures/status?${q}`)
    },
    submitWallet(body: Record<string, unknown>) {
      return api<SubmitResponse>('/api/v1/signatures/wallet', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
    },
    submitTelegram(body: Record<string, unknown>) {
      return api<SubmitResponse>('/api/v1/signatures/telegram', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
    },
  }
}

export type RedirectSanitizeOptions = {
  allowlist: readonly string[]
  allowLocalhost?: boolean
}

function isLoopbackHost(hostname: string): boolean {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '[::1]' || hostname === '::1'
}

export function isAllowedRedirectUri(uri: string, options: RedirectSanitizeOptions): boolean {
  try {
    const parsed = new URL(uri)
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false
    if (options.allowlist.includes(parsed.origin)) return true
    if (options.allowLocalhost && isLoopbackHost(parsed.hostname)) return true
    return false
  } catch {
    return false
  }
}

export function sanitizeRedirectUri(uri: string, options: RedirectSanitizeOptions): string | null {
  if (!isAllowedRedirectUri(uri, options)) return null
  try {
    const parsed = new URL(uri)
    parsed.hash = ''
    return parsed.toString()
  } catch {
    return null
  }
}

export type SignUrlOptions = {
  redirectUri?: string
  appName?: string
  /** Connected wallet for portal continuity. Never a redirect target. */
  account?: string
  property?: string
}

export function buildSignUrl(baseUrl: string, opts: SignUrlOptions = {}): string {
  const url = new URL(baseUrl)
  if (opts.property) url.searchParams.set('property', opts.property)
  if (opts.redirectUri) url.searchParams.set('redirect_uri', opts.redirectUri)
  if (opts.appName) url.searchParams.set('app_name', opts.appName)
  if (opts.account?.trim()) url.searchParams.set('account', opts.account.trim())
  return url.toString()
}
