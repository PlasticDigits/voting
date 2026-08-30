import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { useWalletConnectPairingStore } from '@/hooks/useWalletConnectPairingStore'
import WalletConnectPairingModal from '../WalletConnectPairingModal'

const WC_V1 = 'wc:00e46b69-d0cc-4b3e-b6a2-cee442f97188@1?bridge=https%3A%2F%2Fwalletconnect.luncdash.com&key=abc'

const galaxyAndroid =
  'https://station.hexxagon.io/wcV2#Intent;package=io.hexxagon.station.wallet;scheme=galaxystation;end;'

describe('WalletConnectPairingModal (voting #12 / DEX #519)', () => {
  beforeEach(() => {
    useWalletConnectPairingStore.setState({ isOpen: false, payload: null })
    vi.stubGlobal('navigator', {
      ...navigator,
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
      userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)',
    })
  })

  it('renders nothing when the pairing store is closed', () => {
    render(<WalletConnectPairingModal />)
    expect(screen.queryByTestId('walletconnect-pairing-modal')).not.toBeInTheDocument()
  })

  it('shows Open / Copy / Cancel without a QR canvas', () => {
    useWalletConnectPairingStore.setState({
      isOpen: true,
      payload: {
        uri: WC_V1,
        name: 'LUNC Dash',
        android: '',
        ios: '',
        isStation: true,
        isLuncDash: true,
      },
    })
    render(<WalletConnectPairingModal />)

    expect(screen.getByTestId('walletconnect-pairing-modal')).toBeInTheDocument()
    const openWallet = screen.getByTestId('walletconnect-pairing-wallet')
    expect(openWallet).toHaveTextContent('Open LUNC Dash')
    expect(openWallet.getAttribute('href')?.startsWith('luncdash://')).toBe(true)

    const generic = screen.getByTestId('walletconnect-pairing-generic')
    expect(generic).toHaveTextContent('Open wallet')
    expect(generic).toHaveAttribute('href', WC_V1)

    expect(screen.getByTestId('walletconnect-pairing-copy')).toHaveTextContent('Copy pairing link')
    expect(document.querySelector('canvas')).toBeNull()
    expect(screen.queryByText(/scan/i)).not.toBeInTheDocument()
  })

  it('stacks the pairing portal above Connect Wallet (z-[10001])', () => {
    useWalletConnectPairingStore.setState({
      isOpen: true,
      payload: {
        uri: WC_V1,
        name: 'LUNC Dash',
        android: '',
        ios: '',
        isStation: true,
        isLuncDash: true,
      },
    })
    render(<WalletConnectPairingModal />)
    const portal = screen.getByTestId('walletconnect-pairing-portal')
    expect(portal.className).toContain('z-[10001]')
    expect(screen.getByTestId('walletconnect-pairing-cancel')).toHaveTextContent('Cancel')
  })

  it('does not dismiss when Open or Copy is used', () => {
    useWalletConnectPairingStore.setState({
      isOpen: true,
      payload: {
        uri: WC_V1,
        name: 'LUNC Dash',
        android: '',
        ios: '',
        isStation: true,
        isLuncDash: true,
      },
    })
    render(<WalletConnectPairingModal />)
    fireEvent.click(screen.getByTestId('walletconnect-pairing-wallet'))
    fireEvent.click(screen.getByTestId('walletconnect-pairing-copy'))
    expect(screen.getByTestId('walletconnect-pairing-modal')).toBeInTheDocument()
    expect(useWalletConnectPairingStore.getState().isOpen).toBe(true)
  })

  it('omits an Open href that fails the pairing allowlist', () => {
    useWalletConnectPairingStore.setState({
      isOpen: true,
      payload: {
        uri: WC_V1,
        name: 'Evil',
        android: 'javascript:alert(1)',
        ios: 'javascript:alert(1)',
        isStation: false,
        isLuncDash: false,
      },
    })
    render(<WalletConnectPairingModal />)
    expect(screen.queryByTestId('walletconnect-pairing-wallet')).not.toBeInTheDocument()
    expect(screen.getByTestId('walletconnect-pairing-generic')).toHaveAttribute('href', WC_V1)
  })

  it('Galaxy Android Open uses intent:// not a Hexxagon website', () => {
    vi.stubGlobal('navigator', {
      ...navigator,
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
      userAgent: 'Mozilla/5.0 (Linux; Android 14; Pixel) AppleWebKit/537.36 Chrome/120.0.0.0 Mobile Safari/537.36',
    })
    useWalletConnectPairingStore.setState({
      isOpen: true,
      payload: {
        uri: 'wc:2222222222222222222222222222222222222222222222222222222222222222@2?relay-protocol=irn&symKey=test',
        name: 'Galaxy Station',
        android: galaxyAndroid,
        ios: 'https://station.hexxagon.io/wcV2',
        isStation: false,
        isLuncDash: false,
      },
    })
    render(<WalletConnectPairingModal />)
    const openWallet = screen.getByTestId('walletconnect-pairing-wallet')
    expect(openWallet.getAttribute('href')?.startsWith('intent://')).toBe(true)
    expect(openWallet.getAttribute('href')).toContain('scheme=galaxystation')
    expect(openWallet.getAttribute('href')?.startsWith('https://station.hexxagon.io')).toBe(false)
  })
})
