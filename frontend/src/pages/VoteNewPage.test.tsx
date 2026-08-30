import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import VoteNewPage from '@/pages/VoteNewPage'
import { MIN_PROPOSAL_RAW } from '@/utils/constants'

const identity = {
  address: 'terra1proposerxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
  chain: 'terra' as const,
  legalNetwork: 'TerraClassic' as const,
  label: 'Terra Classic',
}
const snapshot = {
  status: 'registered' as 'idle' | 'unregistered' | 'pending' | 'registered',
  balance: '0',
  asOfHeight: 1,
  error: null as string | null,
  loading: false,
  refresh: async () => undefined,
  completeRegister: async () => undefined,
}

vi.mock('@/hooks/useConnectedIdentity', () => ({
  useConnectedIdentity: () => identity,
}))

vi.mock('@/hooks/useVotingSnapshot', () => ({
  useVotingSnapshot: () => snapshot,
}))

vi.mock('@/services/operatorVoting', () => ({
  createProposal: vi.fn(),
}))

const SECTION = 'This draft section has forty visible characters.'

function fillRequiredSections() {
  for (const key of ['problem', 'solution', 'pros_cons', 'summary', 'success_criteria']) {
    fireEvent.change(screen.getByTestId(`section-${key}`), { target: { value: SECTION } })
  }
}

describe('propose gate', () => {
  it('disables submit below 1000 CL8Y', async () => {
    snapshot.status = 'registered'
    snapshot.balance = (MIN_PROPOSAL_RAW - 1n).toString()
    snapshot.error = null
    render(
      <MemoryRouter>
        <VoteNewPage />
      </MemoryRouter>
    )
    const button = await screen.findByTestId('submit-proposal')
    expect(button).toBeDisabled()
    expect(screen.getByTestId('propose-balance')).toHaveTextContent('need 1000')
  })

  it('enables submit at the 1000 CL8Y boundary once titled and templated', async () => {
    snapshot.status = 'registered'
    snapshot.balance = MIN_PROPOSAL_RAW.toString()
    snapshot.error = null
    render(
      <MemoryRouter>
        <VoteNewPage />
      </MemoryRouter>
    )
    const input = await screen.findByTestId('proposal-title')
    fireEvent.change(input, { target: { value: 'Boundary' } })
    expect(screen.getByTestId('submit-proposal')).toBeDisabled()
    fillRequiredSections()
    await waitFor(() => {
      expect(screen.getByTestId('submit-proposal')).toBeEnabled()
    })
  })

  it('does not treat an unregistered ledger 0 as a live balance', async () => {
    snapshot.status = 'unregistered'
    snapshot.balance = null
    snapshot.error = null
    render(
      <MemoryRouter>
        <VoteNewPage />
      </MemoryRouter>
    )
    expect(screen.queryByTestId('propose-balance')).toBeNull()
    expect(screen.getByTestId('propose-register-hint')).toBeInTheDocument()
    expect(await screen.findByTestId('submit-proposal')).toBeDisabled()
  })

  it('fail-closes propose when the API errors', async () => {
    snapshot.status = 'unregistered'
    snapshot.balance = null
    snapshot.error = 'operator down'
    render(
      <MemoryRouter>
        <VoteNewPage />
      </MemoryRouter>
    )
    expect(screen.getByRole('alert')).toHaveTextContent('operator down')
    expect(await screen.findByTestId('submit-proposal')).toBeDisabled()
    expect(screen.queryByTestId('propose-balance')).toBeNull()
  })
})
