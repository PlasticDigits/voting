/**
 * WalletConnect same-device mobile pairing helpers (GitLab #519).
 *
 * Cosmes `QRCodeModal` auto-`location.href`s after an async session create, which is
 * not a user gesture — iOS/Android often ignore it and leave a QR the user cannot
 * scan on the same phone. Deep-link buttons + copy of the raw `wc:` URI fix that.
 */

export const WALLETCONNECT_MOBILE_VIEWPORT_MAX_PX = 767

export const WC_PAIRING_HOOK_KEY = '__CL8Y_WC_PAIRING_MODAL__'

export type WalletConnectPairingDetails = {
  name: string
  android: string
  ios: string
  isStation: boolean
  isLuncDash: boolean
}

export type WalletConnectPairingHookPayload = WalletConnectPairingDetails & {
  uri: string
}

export type WalletConnectPairingHook = {
  /** Return true to suppress the cosmes QR overlay (mobile path). */
  open: (payload: WalletConnectPairingHookPayload) => boolean
  close: () => void
}

export type WalletConnectDeepLink = {
  id: 'wallet' | 'generic'
  label: string
  href: string
}

export type WalletConnectMobileEnv = {
  userAgent?: string
  platform?: string
  maxTouchPoints?: number
  matchMedia?: (query: string) => Pick<MediaQueryList, 'matches'>
}

const MOBILE_UA = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini|Mobile|Silk/i

/** WC v1 (`…@1?bridge=`) and v2 (`…@2?relay-protocol=`) pairing URIs. */
export function isWalletConnectPairingUri(uri: string): boolean {
  const trimmed = uri.trim()
  if (!trimmed.startsWith('wc:')) return false
  return /@\d/.test(trimmed)
}

/**
 * Mobile UA, iPad desktop-UA, coarse pointer, or viewport ≤767px.
 * Matches issue #519 “mobile viewports/user-agents”.
 */
export function isWalletConnectMobileClient(env?: WalletConnectMobileEnv): boolean {
  const ua = env?.userAgent ?? (typeof navigator !== 'undefined' ? navigator.userAgent : '')
  if (MOBILE_UA.test(ua)) return true

  const platform = env?.platform ?? (typeof navigator !== 'undefined' ? navigator.platform : '')
  const maxTouchPoints = env?.maxTouchPoints ?? (typeof navigator !== 'undefined' ? navigator.maxTouchPoints : 0)
  if (platform === 'MacIntel' && maxTouchPoints > 1) return true

  const matchMedia = env?.matchMedia ?? (typeof window !== 'undefined' ? window.matchMedia.bind(window) : undefined)
  if (!matchMedia) return false
  if (matchMedia(`(max-width: ${WALLETCONNECT_MOBILE_VIEWPORT_MAX_PX}px)`).matches) return true
  if (matchMedia('(pointer: coarse)').matches && matchMedia('(max-width: 1024px)').matches) {
    return true
  }
  return false
}

export function buildLuncDashDeepLink(uri: string): string {
  return `luncdash://wallet_connect?${encodeURIComponent(`payload=${encodeURIComponent(uri)}`)}`
}

/**
 * Cosmes Galaxy Station `android` is `https://host/path#Intent;package=…;scheme=galaxystation;end;`.
 * Chrome Android treats that as a website (the `#Intent` part is a fragment). Convert to `intent://`
 * so the installed app opens (GitLab #554).
 */
export function toAndroidIntentUri(androidTemplate: string): string {
  const trimmed = androidTemplate.trim()
  if (!trimmed) return trimmed
  if (/^intent:/i.test(trimmed)) return trimmed
  const hashIndex = trimmed.indexOf('#Intent')
  if (hashIndex < 0) return trimmed
  const before = trimmed.slice(0, hashIndex)
  const intentPart = trimmed.slice(hashIndex)
  if (!/^https?:\/\//i.test(before)) return trimmed
  const schemeMatch = intentPart.match(/scheme=([^;]+)/i)
  const scheme = schemeMatch?.[1]?.trim()
  try {
    const url = new URL(before)
    const path = url.pathname.replace(/^\//, '')
    if (scheme && scheme !== 'http' && scheme !== 'https') {
      return `intent://${path}${intentPart}`
    }
    return `intent://${url.host}${url.pathname}${url.search}${intentPart}`
  } catch {
    return trimmed
  }
}

export function buildAndroidWalletIntent(androidTemplate: string, uri: string): string {
  const normalized = toAndroidIntentUri(androidTemplate)
  const hashIndex = normalized.indexOf('#')
  if (hashIndex < 0) {
    const sep = normalized.includes('?') ? '&' : '?'
    return `${normalized}${sep}${encodeURIComponent(uri)}`
  }
  return `${normalized.slice(0, hashIndex)}?${encodeURIComponent(uri)}${normalized.slice(hashIndex)}`
}

export function buildIosWalletIntent(iosTemplate: string, uri: string): string {
  const sep = iosTemplate.includes('?') ? '&' : '?'
  return `${iosTemplate}${sep}${encodeURIComponent(uri)}`
}

export function isAndroidUserAgent(userAgent?: string): boolean {
  const ua = userAgent ?? (typeof navigator !== 'undefined' ? navigator.userAgent : '')
  return /Android/i.test(ua)
}

/**
 * Allowlisted schemes/hosts only — pairing hrefs are opened from the dApp chrome.
 * Do not pass through arbitrary URLs from the WalletConnect payload.
 *
 * Cosmostation mobile (cosmes `CosmostationController`): Android already ships
 * `intent://…scheme=cosmostation` (**WC-M5** `intent:`); iOS is `cosmostation://wc`
 * and must be listed here ([#566](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/566)).
 */
export function isAllowedWalletConnectDeepLink(href: string): boolean {
  return /^(wc:|luncdash:|keplrwallet:|galaxystation:|cosmostation:|intent:|https:\/\/station\.hexxagon\.io\/|https:\/\/terrastation\.page\.link\/)/i.test(
    href
  )
}

export function buildWalletConnectDeepLinks(
  details: WalletConnectPairingDetails,
  uri: string,
  env?: Pick<WalletConnectMobileEnv, 'userAgent'>
): WalletConnectDeepLink[] {
  const links: WalletConnectDeepLink[] = []
  const walletHref = walletSpecificDeepLink(details, uri, env)
  if (walletHref && isAllowedWalletConnectDeepLink(walletHref)) {
    links.push({
      id: 'wallet',
      label: `Open ${details.name}`,
      href: walletHref,
    })
  }
  if (isAllowedWalletConnectDeepLink(uri)) {
    links.push({
      id: 'generic',
      label: 'Open wallet',
      href: uri,
    })
  }
  return links
}

function walletSpecificDeepLink(
  details: WalletConnectPairingDetails,
  uri: string,
  env?: Pick<WalletConnectMobileEnv, 'userAgent'>
): string | null {
  if (details.isStation && details.isLuncDash) {
    return buildLuncDashDeepLink(uri)
  }
  if (details.isStation) {
    return `https://terrastation.page.link/?link=https://terra.money?${encodeURIComponent(
      `action=wallet_connect&payload=${encodeURIComponent(uri)}`
    )}&apn=money.terra.station&ibi=money.terra.station&isi=1548434735`
  }
  if (isAndroidUserAgent(env?.userAgent) && details.android.trim()) {
    return buildAndroidWalletIntent(details.android, uri)
  }
  if (details.ios.trim()) {
    return buildIosWalletIntent(details.ios, uri)
  }
  return null
}

export function getWalletConnectPairingHook(): WalletConnectPairingHook | undefined {
  const bag = globalThis as typeof globalThis & Record<string, unknown>
  const hook = bag[WC_PAIRING_HOOK_KEY]
  if (!hook || typeof hook !== 'object') return undefined
  const candidate = hook as Partial<WalletConnectPairingHook>
  if (typeof candidate.open !== 'function' || typeof candidate.close !== 'function') return undefined
  return candidate as WalletConnectPairingHook
}
