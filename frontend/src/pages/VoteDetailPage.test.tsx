import { describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import VoteDetailPage from '@/pages/VoteDetailPage'
import type { ProposalDetail } from '@/services/operatorVoting'

const identity = {
  address: 'terra1voterxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
  chain: 'terra' as const,
  legalNetwork: 'TerraClassic' as const,
  label: 'Terra Classic',
}

const base: ProposalDetail = {
  id: '11111111-1111-1111-1111-111111111111',
  chain: 'terra',
  proposer: 'terra1proposerxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
  title: 'Draft idea',
  summary: 'A summary with forty visible characters for the list card.',
  terra_height: null,
  bsc_block: null,
  created_at: '2026-08-30T00:00:00Z',
  status: 'draft',
  tally: [],
  body_html: '<p>legacy</p>',
  body_sections: {
    context: '',
    problem: '<p>problem text that is long enough for the template minimum.</p>',
    solution: '<p>solution text that is long enough for the template minimum.</p>',
    pros_cons: '<p>tradeoffs text that is long enough for the template minimum.</p>',
    summary: '<p>A summary with forty visible characters for the list card.</p>',
    success_criteria: '<p>success text that is long enough for the template minimum.</p>',
  },
  comments: [],
  analysis: [],
  advisory: true,
}

let current: ProposalDetail = { ...base }

vi.mock('@/hooks/useConnectedIdentity', () => ({
  useConnectedIdentity: () => identity,
}))

vi.mock('@/services/operatorVoting', () => ({
  getProposal: async () => current,
  castVote: vi.fn(),
  postComment: vi.fn(),
  postAnalysis: vi.fn(),
  openProposal: vi.fn(),
  amendProposal: vi.fn(),
}))

function renderDetail() {
  return render(
    <MemoryRouter initialEntries={[`/${base.id}`]}>
      <Routes>
        <Route path="/:id" element={<VoteDetailPage />} />
      </Routes>
    </MemoryRouter>
  )
}

describe('draft vs open vote CTAs', () => {
  it('hides vote buttons on a draft', async () => {
    current = { ...base, status: 'draft', terra_height: null, bsc_block: null }
    renderDetail()
    await waitFor(() => expect(screen.getByTestId('proposal-status')).toHaveTextContent('draft'))
    expect(screen.queryByTestId('vote-actions')).toBeNull()
    expect(screen.getByTestId('open-poll')).toBeInTheDocument()
  })

  it('shows vote buttons after open', async () => {
    current = { ...base, status: 'open', terra_height: 40, bsc_block: 1 }
    renderDetail()
    await waitFor(() => expect(screen.getByTestId('vote-actions')).toBeInTheDocument())
    expect(screen.getByTestId('vote-for')).toBeEnabled()
    expect(screen.queryByTestId('open-poll')).toBeNull()
  })
})
