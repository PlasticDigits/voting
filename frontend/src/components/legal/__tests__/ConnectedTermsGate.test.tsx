import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import ConnectedTermsGate from '@/components/legal/ConnectedTermsGate'
import { useWalletStore } from '@/hooks/useWallet'
import { useEvmWalletStore } from '@/stores/evmWallet'

const getSignatureStatus = vi.fn()
const getTermsLatest = vi.fn()

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

describe('ConnectedTermsGate', () => {
  beforeEach(() => {
    getSignatureStatus.mockReset()
    getTermsLatest.mockReset()
    getTermsLatest.mockResolvedValue(null)
    useWalletStore.setState({ address: null, walletType: null, isConnecting: false, error: null })
    useEvmWalletStore.setState({ address: null, connected: false, connectorId: null })
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
    getSignatureStatus.mockResolvedValue({
      property: 'vote.cl8y.com',
      latest_version: '1.0.0',
      signed_latest: false,
      signed_version: null,
      signed_at: null,
    })
    getTermsLatest.mockResolvedValue({
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
    })

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

  it('uses EVM network for a connected 0x address', async () => {
    useEvmWalletStore.setState({
      address: '0x1111111111111111111111111111111111111111',
      connected: true,
      connectorId: 'mock',
    })
    getSignatureStatus.mockResolvedValue({
      property: 'vote.cl8y.com',
      latest_version: '1.0.0',
      signed_latest: true,
      signed_version: '1.0.0',
      signed_at: '2026-01-02T00:00:00Z',
    })

    render(
      <ConnectedTermsGate>
        <button type="button">Vote CTA</button>
      </ConnectedTermsGate>
    )

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /vote cta/i })).toBeVisible()
    })
    expect(getSignatureStatus).toHaveBeenCalledWith(
      'vote.cl8y.com',
      'EVM',
      '0x1111111111111111111111111111111111111111'
    )
  })
})
