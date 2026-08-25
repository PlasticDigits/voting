/**
 * Wallet / WebView in-app browser detect (GitLab #554, WC-M7).
 * Alternate connect path — not a substitute for same-device Open / Copy.
 */

export type WalletInAppBrowserInfo = {
  isInAppBrowser: boolean
  browserName: string | null
}

const WALLET_UA_PATTERNS: [RegExp, string][] = [
  [/Keplr/i, 'Keplr'],
  [/LuncDash|LUNCDash|LUNC Dash/i, 'Lunc Dash'],
  [/GalaxyStation|Galaxy Station/i, 'Galaxy Station'],
]

export function detectWalletInAppBrowser(
  userAgent = typeof navigator !== 'undefined' ? navigator.userAgent : ''
): WalletInAppBrowserInfo {
  const ua = userAgent || ''
  for (const [pattern, name] of WALLET_UA_PATTERNS) {
    if (pattern.test(ua)) return { isInAppBrowser: true, browserName: name }
  }
  if (/; wv\)/.test(ua)) return { isInAppBrowser: true, browserName: 'WebView' }
  if (/iPhone|iPad|iPod/.test(ua) && /AppleWebKit/.test(ua) && !/Safari/.test(ua)) {
    return { isInAppBrowser: true, browserName: 'In-App Browser' }
  }
  return { isInAppBrowser: false, browserName: null }
}
