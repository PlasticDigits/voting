import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import VoteDetailPage from '@/pages/VoteDetailPage'
import { GOLDEN_SECTIONS } from '@/utils/proposalSections'

vi.mock('@/hooks/useConnectedIdentity', () => ({
  useConnectedIdentity: () => ({ address: null, chain: null, label: '' }),
}))

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { ...actual, useParams: () => ({ id: '11111111-1111-1111-1111-111111111111' }) }
})

const getProposal = vi.fn()

vi.mock('@/services/operatorVoting', () => ({
  getProposal: (...args: unknown[]) => getProposal(...args),
  castVote: vi.fn(),
}))

describe('proposal detail sections', () => {
  it('renders labeled sections in canonical order', async () => {
    getProposal.mockResolvedValue({
      id: '11111111-1111-1111-1111-111111111111',
      chain: 'terra',
      proposer: 'terra1x',
      title: 'Templated',
      terra_height: 1,
      bsc_block: 1,
      created_at: new Date().toISOString(),
      status: 'open',
      tally: [],
      advisory: true,
      body_html: '<p>concat</p>',
      body_sections: GOLDEN_SECTIONS,
    })
    render(
      <MemoryRouter>
        <VoteDetailPage />
      </MemoryRouter>
    )
    expect(await screen.findByTestId('proposal-sections')).toBeInTheDocument()
    expect(screen.getByTestId('proposal-section-view-summary')).toHaveTextContent('Summary (TL;DR)')
    expect(screen.getByTestId('proposal-section-view-problem')).toHaveTextContent(
      'The idea or problem to be solved'
    )
    expect(screen.queryByTestId('proposal-legacy-body')).toBeNull()
  })

  it('falls back to sanitized legacy body_html', async () => {
    getProposal.mockResolvedValue({
      id: '11111111-1111-1111-1111-111111111111',
      chain: 'terra',
      proposer: 'terra1x',
      title: 'Legacy',
      terra_height: 1,
      bsc_block: 1,
      created_at: new Date().toISOString(),
      status: 'open',
      tally: [],
      advisory: true,
      body_html: '<p>old body</p><script>alert(1)</script>',
      body_sections: null,
    })
    render(
      <MemoryRouter>
        <VoteDetailPage />
      </MemoryRouter>
    )
    const legacy = await screen.findByTestId('proposal-legacy-body')
    expect(legacy).toHaveTextContent('old body')
    expect(legacy.innerHTML).not.toMatch(/script/i)
  })
})
