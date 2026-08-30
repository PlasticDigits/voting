import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import ConnectedTermsGate from '@/components/legal/ConnectedTermsGate'
import { useWalletStore } from '@/hooks/useWallet'
import { useEvmWalletStore } from '@/stores/evmWallet'

const getSignatureStatus = vi.fn()
const getTermsLatest = vi.fn()

const EVM_ACCOUNT = '0x1111111111111111111111111111111111111111'
const OTHER_ACCOUNT = '0x2222222222222222222222222222222222222222'

const UNSIGNED_STATUS = {
  property: 'vote.cl8y.com',
  latest_version: '1.0.0',
  signed_latest: false,
  signed_version: null,
  signed_at: null,
}

const SIGNED_STATUS = {
  property: 'vote.cl8y.com',
  latest_version: '1.0.0',
  signed_latest: true,
  signed_version: '1.0.0',
  signed_at: '2026-01-02T00:00:00Z',
}

const TERMS_LATEST = {
  property: 'vote.cl8y.com',
  version_label: '1.0.0',
  effective_date: '2026-01-01',
  content_sha256: 'abc',
  published_at: '2026-01-01T00:00:00Z',
  sign_urls: {
    telegram: 'https://terms.cl8y.com/sign/telegram',
    evm: 'https://terms.cl8y.com/sign/evm',
    terra_classic: 'https://terms.cl8y.com/sign/terra-classic?property=vote.cl8y.com',
    solana: 'https://terms.cl8y.com/sign/solana',
  },
}

vi.mock('@plasticdigits/cl8y-clickwrap', async () => {
  const vendor = await import('@/vendor/cl8y-clickwrap')
  return {
    ...vendor,
    createClient: () => ({
      apiBaseUrl: 'https://api.terms.cl8y.com',
      termsBaseUrl: 'https://terms.cl8y.com',
      getSignatureStatus,
      getTermsLatest,
      getTermsContent: vi.fn(),
      submitWallet: vi.fn(),
      submitTelegram: vi.fn(),
    }),
  }
})

vi.mock('@plasticdigits/cl8y-clickwrap/react', async () => {
  const { TermsGate } = await import('@/vendor/cl8y-clickwrap-react')
  return { TermsGate }
})

vi.mock('@/utils/legalClickwrap', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/utils/legalClickwrap')>()
  return {
    ...actual,
    skipLegalClickwrapForAutomation: () => false,
    getLegalClickwrapClient: () => ({
      apiBaseUrl: 'https://api.terms.cl8y.com',
      termsBaseUrl: 'https://terms.cl8y.com',
      getSignatureStatus,
      getTermsLatest,
      getTermsContent: vi.fn(),
      submitWallet: vi.fn(),
      submitTelegram: vi.fn(),
    }),
  }
})

function connectUnsignedEvm(connectorId: string) {
  useEvmWalletStore.setState({
    address: EVM_ACCOUNT,
    connected: true,
    connectorId,
  })
  getSignatureStatus.mockResolvedValue(UNSIGNED_STATUS)
  getTermsLatest.mockResolvedValue(TERMS_LATEST)
}

describe('ConnectedTermsGate', () => {
  beforeEach(() => {
    getSignatureStatus.mockReset()
    getTermsLatest.mockReset()
    getTermsLatest.mockResolvedValue(null)
    useWalletStore.setState({ address: null, walletType: null, isConnecting: false, error: null })
    useEvmWalletStore.setState({ address: null, connected: false, connectorId: null })
    delete (window as Window & { ethereum?: unknown }).ethereum
  })

  afterEach(() => {
    delete (window as Window & { ethereum?: unknown }).ethereum
    vi.unstubAllGlobals()
  })

  it('renders children when wallet is disconnected (browse OK)', () => {
    render(
      <ConnectedTermsGate>
        <button type="button">Propose CTA</button>
      </ConnectedTermsGate>
    )
    expect(screen.getByRole('button', { name: /propose cta/i })).toBeVisible()
    expect(getSignatureStatus).not.toHaveBeenCalled()
  })

  it('fail-closed: unsigned hides propose/vote CTAs', async () => {
    useWalletStore.setState({ address: 'terra1unsignedexample', walletType: 'keplr' })
    getSignatureStatus.mockResolvedValue(UNSIGNED_STATUS)
    getTermsLatest.mockResolvedValue(TERMS_LATEST)

    render(
      <ConnectedTermsGate>
        <button type="button">Propose CTA</button>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /accept terms/i })).toBeVisible()
    })
    expect(screen.queryByRole('button', { name: /propose cta/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /vote cta/i })).not.toBeInTheDocument()
    expect(getSignatureStatus).toHaveBeenCalledWith('vote.cl8y.com', 'TERRA_CLASSIC', 'terra1unsignedexample')
  })

  it('Terra unsigned without Keplr shows the Keplr hint and no EVM CTAs', async () => {
    useWalletStore.setState({ address: 'terra1unsignedexample', walletType: 'keplr' })
    getSignatureStatus.mockResolvedValue(UNSIGNED_STATUS)
    getTermsLatest.mockResolvedValue(TERMS_LATEST)

    render(
      <ConnectedTermsGate>
        <button type="button">Propose CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByTestId('legal-keplr-inapp-hint')).toBeVisible()
    })
    expect(screen.queryByTestId('legal-evm-inapp-hint')).not.toBeInTheDocument()
    expect(screen.queryByTestId('legal-evm-open-metamask')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /propose cta/i })).not.toBeInTheDocument()
  })

  it('uses EVM network for a connected 0x address', async () => {
    useEvmWalletStore.setState({
      address: EVM_ACCOUNT,
      connected: true,
      connectorId: 'mock',
    })
    getSignatureStatus.mockResolvedValue(SIGNED_STATUS)

    render(
      <ConnectedTermsGate>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /vote cta/i })).toBeVisible()
    })
    expect(getSignatureStatus).toHaveBeenCalledWith('vote.cl8y.com', 'EVM', EVM_ACCOUNT)
    expect(getSignatureStatus).not.toHaveBeenCalledWith('vote.cl8y.com', 'TERRA_CLASSIC', expect.anything())
    expect(screen.queryByTestId('legal-evm-inapp-hint')).not.toBeInTheDocument()
    expect(screen.queryByTestId('legal-keplr-inapp-hint')).not.toBeInTheDocument()
  })

  it('EVM unsigned without inject shows Accept plus MetaMask/copy hint, not Keplr', async () => {
    connectUnsignedEvm('mock')

    render(
      <ConnectedTermsGate>
        <button type="button">Propose CTA</button>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /accept terms/i })).toBeVisible()
      expect(screen.getByTestId('legal-evm-inapp-hint')).toBeVisible()
    })
    expect(screen.queryByTestId('legal-keplr-inapp-hint')).not.toBeInTheDocument()
    expect(screen.getByTestId('legal-evm-open-metamask')).toHaveAttribute(
      'href',
      expect.stringMatching(/^https:\/\/link\.metamask\.io\/dapp\//)
    )
    expect(screen.getByTestId('legal-evm-copy-link')).toBeVisible()
    expect(screen.queryByRole('button', { name: /propose cta/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /vote cta/i })).not.toBeInTheDocument()
    expect(getSignatureStatus).toHaveBeenCalledWith('vote.cl8y.com', 'EVM', EVM_ACCOUNT)
  })

  it('EVM WalletConnect shows the hint even if window.ethereum exists', async () => {
    ;(window as Window & { ethereum?: unknown }).ethereum = { request: vi.fn() }
    connectUnsignedEvm('walletConnect')

    render(
      <ConnectedTermsGate>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByTestId('legal-evm-inapp-hint')).toBeVisible()
    })
    expect(screen.getByRole('button', { name: /accept terms/i })).toBeVisible()
    expect(screen.queryByRole('button', { name: /vote cta/i })).not.toBeInTheDocument()
  })

  it('hides the EVM hint when injected MetaMask is present and not WalletConnect', async () => {
    ;(window as Window & { ethereum?: unknown }).ethereum = { request: vi.fn() }
    connectUnsignedEvm('io.metamask')

    render(
      <ConnectedTermsGate>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /accept terms/i })).toBeVisible()
    })
    expect(screen.queryByTestId('legal-evm-inapp-hint')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /vote cta/i })).not.toBeInTheDocument()
  })

  it('Accept URL includes property, redirect_uri, app_name, and the connected account', async () => {
    const loc = { href: 'http://localhost:5176/' }
    vi.stubGlobal('location', loc)
    connectUnsignedEvm('mock')

    render(
      <ConnectedTermsGate>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    const accept = await screen.findByRole('button', { name: /accept terms/i })
    fireEvent.click(accept)

    const href = loc.href
    expect(href).toContain('sign/evm')
    expect(href).toContain('property=vote.cl8y.com')
    expect(href).toContain('app_name=CL8Y+Voting')
    expect(href).toContain(`account=${EVM_ACCOUNT}`)
    expect(href).toContain('redirect_uri=')
    expect(href).not.toContain(OTHER_ACCOUNT)
    expect(new URL(href).searchParams.get('account')).toBe(EVM_ACCOUNT)
  })

  it('fail-closes propose/vote when Legal status errors', async () => {
    useEvmWalletStore.setState({
      address: EVM_ACCOUNT,
      connected: true,
      connectorId: 'mock',
    })
    getSignatureStatus.mockRejectedValue(new Error('status down'))

    render(
      <ConnectedTermsGate>
        <button type="button">Propose CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeVisible()
    })
    expect(screen.queryByRole('button', { name: /propose cta/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /accept terms/i })).not.toBeInTheDocument()
  })
})
