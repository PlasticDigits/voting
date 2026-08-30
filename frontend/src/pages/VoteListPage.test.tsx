import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import VoteListPage from '@/pages/VoteListPage'

const identity = {
  address: 'terra1holderxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
  chain: 'terra' as const,
  legalNetwork: 'TerraClassic' as const,
  label: 'Terra Classic',
}
const snapshot = {
  status: 'unregistered' as 'idle' | 'unregistered' | 'pending' | 'registered',
  balance: null as string | null,
  asOfHeight: null as number | null,
  error: null as string | null,
  loading: false,
  polling: false,
  lastPollAt: null as number | null,
  canRetry: false,
  refresh: async () => undefined,
  retryPending: async () => undefined,
  completeRegister: async () => undefined,
}

vi.mock('@/hooks/useConnectedIdentity', () => ({
  useConnectedIdentity: () => identity,
}))

vi.mock('@/hooks/useVotingSnapshot', () => ({
  useVotingSnapshot: () => snapshot,
}))

vi.mock('@/services/operatorVoting', () => ({
  listProposals: async () => [],
}))

describe('list page snapshot copy', () => {
  it('does not render 0 CL8Y for an unregistered connection', async () => {
    snapshot.status = 'unregistered'
    snapshot.balance = null
    snapshot.canRetry = false
    snapshot.polling = false
    snapshot.error = null
    render(
      <MemoryRouter>
        <VoteListPage />
      </MemoryRouter>
    )
    const connected = await screen.findByTestId('connected-chain')
    expect(connected).toHaveTextContent('Terra Classic')
    expect(connected).not.toHaveTextContent('0 CL8Y')
    expect(screen.getByTestId('register-cta')).toBeEnabled()
    expect(screen.getByTestId('register-hint')).toBeInTheDocument()
  })

  it('shows pending instead of Registered+0', async () => {
    snapshot.status = 'pending'
    snapshot.balance = null
    snapshot.canRetry = false
    snapshot.polling = true
    snapshot.error = null
    render(
      <MemoryRouter>
        <VoteListPage />
      </MemoryRouter>
    )
    expect(await screen.findByTestId('registration-pending')).toHaveTextContent('Registering')
    expect(screen.queryByTestId('registration-status')).toBeNull()
    expect(screen.queryByTestId('register-retry')).toBeNull()
    expect(screen.getByTestId('connected-chain')).not.toHaveTextContent('CL8Y')
  })

  it('shows Retry after a pending timeout instead of trapping Registering…', async () => {
    snapshot.status = 'pending'
    snapshot.balance = null
    snapshot.canRetry = true
    snapshot.polling = false
    snapshot.error = 'Registration snapshot is still pending. Wait for the ledger, then retry.'
    snapshot.lastPollAt = Date.now() - 5_000
    render(
      <MemoryRouter>
        <VoteListPage />
      </MemoryRouter>
    )
    expect(await screen.findByTestId('register-retry')).toBeEnabled()
    expect(screen.queryByTestId('registration-pending')).toBeNull()
    expect(screen.queryByTestId('register-cta')).toBeNull()
    expect(screen.getByRole('alert')).toHaveTextContent('still pending')
    expect(screen.getByTestId('register-last-poll')).toHaveTextContent('ago')
    expect(screen.getByTestId('connected-chain')).not.toHaveTextContent('CL8Y')
  })

  it('shows the server snapshot after registration', async () => {
    snapshot.status = 'registered'
    snapshot.balance = '3540000000000000000000'
    snapshot.canRetry = false
    snapshot.polling = false
    snapshot.error = null
    snapshot.lastPollAt = null
    render(
      <MemoryRouter>
        <VoteListPage />
      </MemoryRouter>
    )
    expect(await screen.findByTestId('registration-status')).toHaveTextContent('Registered')
    expect(screen.getByTestId('connected-chain')).toHaveTextContent('3540 CL8Y')
  })
})
