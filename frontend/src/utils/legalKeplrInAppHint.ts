/**
 * After WalletConnect, Legal portal still needs `window.keplr` (C1 — DEX must not
 * implement ADR-036). Surface a next step instead of a dead Accept button (#554).
 */
export function shouldShowLegalKeplrInAppHint(input: {
  hasKeplrExtension: boolean
  signedLatest: boolean | null
}): boolean {
  if (input.hasKeplrExtension) return false
  return input.signedLatest === false
}

export const LEGAL_KEPLR_INAPP_HINT = 'Open this site in the Keplr browser to accept terms.'
