import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import VoteNewPage from '@/pages/VoteNewPage'
import { MIN_PROPOSAL_RAW } from '@/utils/constants'

const identity = { address: 'terra1proposerxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx', chain: 'terra' as const, legalNetwork: 'TerraClassic' as const, label: 'Terra Classic' }
const balance = { current: '0' }

vi.mock('@/hooks/useConnectedIdentity', () => ({
  useConnectedIdentity: () => identity,
}))

vi.mock('@/services/operatorVoting', () => ({
  getBalance: async () => ({ chain: 'terra', address: identity.address, height: 1, balance: balance.current }),
  createProposal: vi.fn(),
}))

vi.mock('@tiptap/react', () => ({
  useEditor: () => ({ getHTML: () => '<p>hello</p>' }),
  EditorContent: () => <div data-testid="proposal-body" />,
}))

vi.mock('@tiptap/starter-kit', () => ({ default: {} }))

describe('propose gate', () => {
  it('disables submit below 1000 CL8Y', async () => {
    balance.current = (MIN_PROPOSAL_RAW - 1n).toString()
    render(
      <MemoryRouter>
        <VoteNewPage />
      </MemoryRouter>
    )
    const button = await screen.findByTestId('submit-proposal')
    expect(button).toBeDisabled()
  })

  it('enables submit at the 1000 CL8Y boundary once titled', async () => {
    balance.current = MIN_PROPOSAL_RAW.toString()
    render(
      <MemoryRouter>
        <VoteNewPage />
      </MemoryRouter>
    )
    const input = await screen.findByTestId('proposal-title')
    fireEvent.change(input, { target: { value: 'Boundary' } })
    await waitFor(() => {
      expect(screen.getByTestId('submit-proposal')).toBeEnabled()
    })
  })
})
