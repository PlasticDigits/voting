import { describe, expect, it } from 'vitest'
import { WalletName, WalletType } from '@goblinhunt/cosmes/wallet'
import {
  resolveConnectWalletOptions,
  shouldOfferCosmostationWalletConnect,
  shouldOfferKeplrWalletConnect,
  shouldOfferStationWalletConnect,
  type ConnectWalletOptionEnv,
} from '../connectWalletOptions'

const desktop: ConnectWalletOptionEnv = {
  isMobileClient: false,
  keplrInjected: false,
  stationInjected: false,
  cosmostationInjected: false,
}

const mobileNone: ConnectWalletOptionEnv = {
  isMobileClient: true,
  keplrInjected: false,
  stationInjected: false,
  cosmostationInjected: false,
}

function row(env: ConnectWalletOptionEnv, walletName: WalletName) {
  return resolveConnectWalletOptions(env).find((option) => option.walletName === walletName)
}

describe('resolveConnectWalletOptions (GitLab #554 / #566)', () => {
  it('offers Keplr WalletConnect on mobile when window.keplr is absent', () => {
    expect(shouldOfferKeplrWalletConnect(mobileNone)).toBe(true)
    const keplr = row(mobileNone, WalletName.KEPLR)
    expect(keplr?.walletType).toBe(WalletType.WALLETCONNECT)
    expect(keplr?.connectionLabel).toBe('WalletConnect')
  })

  it('offers Station WalletConnect on mobile when station is not injected', () => {
    expect(shouldOfferStationWalletConnect(mobileNone)).toBe(true)
    const station = row(mobileNone, WalletName.STATION)
    expect(station?.walletType).toBe(WalletType.WALLETCONNECT)
    expect(station?.connectionLabel).toBe('WalletConnect')
  })

  it('offers Cosmostation WalletConnect on mobile when Cosmostation is not injected', () => {
    expect(shouldOfferCosmostationWalletConnect(mobileNone)).toBe(true)
    const cosmo = row(mobileNone, WalletName.COSMOSTATION)
    expect(cosmo?.walletType).toBe(WalletType.WALLETCONNECT)
    expect(cosmo?.connectionLabel).toBe('WalletConnect')
  })

  it('keeps Keplr Extension when injected (in-app browser, WC-M7)', () => {
    const env = { ...mobileNone, keplrInjected: true }
    expect(shouldOfferKeplrWalletConnect(env)).toBe(false)
    expect(row(env, WalletName.KEPLR)?.walletType).toBe(WalletType.EXTENSION)
  })

  it('keeps Station Extension when injected (in-app browser, WC-M7)', () => {
    const env = { ...mobileNone, stationInjected: true }
    expect(shouldOfferStationWalletConnect(env)).toBe(false)
    expect(row(env, WalletName.STATION)?.walletType).toBe(WalletType.EXTENSION)
  })

  it('keeps Cosmostation Extension when injected (in-app browser, WC-M7)', () => {
    const env = { ...mobileNone, cosmostationInjected: true }
    expect(shouldOfferCosmostationWalletConnect(env)).toBe(false)
    expect(row(env, WalletName.COSMOSTATION)?.walletType).toBe(WalletType.EXTENSION)
  })

  it('keeps desktop Keplr / Station / Cosmostation as Extension even without injection', () => {
    expect(row(desktop, WalletName.KEPLR)?.walletType).toBe(WalletType.EXTENSION)
    expect(row(desktop, WalletName.STATION)?.walletType).toBe(WalletType.EXTENSION)
    expect(row(desktop, WalletName.COSMOSTATION)?.walletType).toBe(WalletType.EXTENSION)
  })

  it('never lists Leap (GitLab #159)', () => {
    const names = resolveConnectWalletOptions(mobileNone).map((option) => option.walletName)
    expect(names).not.toContain(WalletName.LEAP)
    expect(names).toEqual([
      WalletName.STATION,
      WalletName.KEPLR,
      WalletName.COSMOSTATION,
      WalletName.LUNCDASH,
      WalletName.GALAXYSTATION,
    ])
  })
})
