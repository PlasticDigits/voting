import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { signVotingRequest } from '@/services/votingSign'
import VoteDetailPage from '@/pages/VoteDetailPage'
import type { ProposalDetail } from '@/services/operatorVoting'

const identity = {
  address: 'terra1voterxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
  chain: 'terra' as const,
  legalNetwork: 'TerraClassic' as const,
  label: 'Terra Classic',
}

const base: ProposalDetail = {
  revision: 1,
  snapshot_validity: 'valid',
  snapshot_incident: null,
  body_hash: 'a'.repeat(64),
  opened_at: null,
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

let currentWeight = {
  proposal_id: base.id,
  address: identity.address,
  chain: 'terra' as const,
  weight: '1000000000000000000000',
  eligible: true,
}
beforeEach(() => {
  currentWeight = {
    proposal_id: base.id,
    address: identity.address,
    chain: 'terra',
    weight: '1000000000000000000000',
    eligible: true,
  }
  vi.mocked(signVotingRequest).mockClear()
})
vi.mock('@/services/votingSign', () => ({ signVotingRequest: vi.fn() }))

let current: ProposalDetail = { ...base }

vi.mock('@/hooks/useConnectedIdentity', () => ({
  useConnectedIdentity: () => identity,
}))

vi.mock('@/services/operatorVoting', () => ({
  getProposal: async () => current,
  getFrozenWeight: async () => currentWeight,
  castVote: vi.fn(),
  postComment: vi.fn(),
  postAnalysis: vi.fn(),
  openProposal: vi.fn(),
  amendProposal: vi.fn(),
  canonicalizeForSigning: vi.fn(),
}))

function renderDetail() {
  return render(
    <MemoryRouter initialEntries={[`/${base.id}`]}>
      <Routes>
        <Route path="/:id" element={<VoteDetailPage />} />
      </Routes>
    </MemoryRouter>,
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
    await waitFor(() => expect(screen.getByTestId('vote-for')).toBeEnabled())
    expect(screen.queryByTestId('open-poll')).toBeNull()
  })
})

it('sanitizes hostile imported comments and analysis through the actual detail renderer', async () => {
  const attack =
    '<div><a href="javascript:alert(1)" onclick="alert(2)">safe text</a><svg onload="alert(3)"></svg></div>'
  current = {
    ...base,
    comments: [
      {
        id: 'comment',
        chain: 'bsc',
        wallet_address: '0x1111111111111111111111111111111111111111',
        body_html: attack,
        created_at: base.created_at,
      },
    ],
    analysis: [
      {
        id: 'analysis',
        chain: 'terra',
        wallet_address: identity.address,
        body_html: attack,
        source: 'committee',
        revision: 1,
        created_at: base.created_at,
        machine_generated: false,
        sections: {
          benefits: attack,
          long_term: attack,
          risks: attack,
          short_term: attack,
          what: attack,
        },
      },
    ],
  }
  const { container } = renderDetail()
  await screen.findByTestId('analysis-item')
  expect(container.querySelector('svg,script')).toBeNull()
  for (const el of container.querySelectorAll('*')) {
    for (const attr of el.attributes) expect(attr.name).not.toMatch(/^on/)
    if (el.hasAttribute('href')) expect(el.getAttribute('href')).not.toMatch(/^javascript:/)
  }
})

for (const validity of ['invalid', 'legacy_unverified'] as const) {
  it(`retains history and disables voting for ${validity} snapshots`, async () => {
    current = {
      ...base,
      status: 'open',
      snapshot_validity: validity,
      snapshot_incident: 'canonical source fork',
      tally: [{ choice: 'for', weight: '1000000000000000000000', voters: 1 }],
    }
    renderDetail()
    const notice = await screen.findByTestId('snapshot-review-notice')
    expect(notice).toHaveTextContent('New votes are disabled')
    expect(screen.getByTestId('snapshot-incident')).toHaveTextContent('canonical source fork')
    expect(screen.queryByTestId('vote-actions')).toBeNull()
    expect(screen.getByTestId('proposal-tally')).toHaveTextContent('1000')
    expect(screen.getByTestId('proposal-sections')).toBeInTheDocument()
  })
}

it('disables signing for a later registration with zero stored weight in the older poll', async () => {
  current = { ...base, status: 'open', terra_height: 40, bsc_block: 85 }
  currentWeight = { ...currentWeight, weight: '0', eligible: false }
  renderDetail()
  await waitFor(() =>
    expect(screen.getByTestId('frozen-weight-status')).toHaveTextContent(
      'no eligible voting weight',
    ),
  )
  expect(screen.getByTestId('vote-for')).toBeDisabled()
  expect(screen.getByTestId('frozen-weight-status')).toHaveTextContent(
    'existing frozen weights stay unchanged',
  )
  expect(signVotingRequest).not.toHaveBeenCalled()
})
it('rechecks exact frozen weight before asking the wallet to sign', async () => {
  current = { ...base, status: 'open', terra_height: 40, bsc_block: 85 }
  renderDetail()
  await waitFor(() => expect(screen.getByTestId('vote-for')).toBeEnabled())
  currentWeight = { ...currentWeight, weight: '0', eligible: false }
  fireEvent.click(screen.getByTestId('vote-for'))
  await screen.findByRole('alert')
  expect(signVotingRequest).not.toHaveBeenCalled()
  expect(screen.getByTestId('vote-for')).toBeDisabled()
})
