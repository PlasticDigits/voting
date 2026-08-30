import { describe, expect, it } from 'vitest'
import { LEGAL_KEPLR_INAPP_HINT, shouldShowLegalKeplrInAppHint } from '../legalKeplrInAppHint'

describe('shouldShowLegalKeplrInAppHint', () => {
  it('shows only when unsigned and no Keplr extension is present', () => {
    expect(shouldShowLegalKeplrInAppHint({ hasKeplrExtension: false, signedLatest: false })).toBe(true)
    expect(shouldShowLegalKeplrInAppHint({ hasKeplrExtension: true, signedLatest: false })).toBe(false)
    expect(shouldShowLegalKeplrInAppHint({ hasKeplrExtension: false, signedLatest: true })).toBe(false)
    expect(shouldShowLegalKeplrInAppHint({ hasKeplrExtension: false, signedLatest: null })).toBe(false)
  })

  it('keeps Terra copy and does not mention MetaMask or Binance', () => {
    expect(LEGAL_KEPLR_INAPP_HINT).toMatch(/Keplr/)
    expect(LEGAL_KEPLR_INAPP_HINT).not.toMatch(/MetaMask|Binance/)
  })
})
