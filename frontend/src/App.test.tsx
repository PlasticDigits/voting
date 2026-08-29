import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it, vi } from 'vitest'
import App from './App'
import { ROUTES, VOTE_PREFIX_ALIASES } from './routes'

vi.mock('@/pages/VoteListPage', () => ({
  default: () => <div data-testid="list-page">list</div>,
}))

vi.mock('@/pages/VoteNewPage', () => ({
  default: () => <div data-testid="new-page">new</div>,
}))

vi.mock('@/pages/VoteDetailPage', () => ({
  default: () => <div data-testid="detail-page">detail</div>,
}))

vi.mock('@/components/layout/AppLayout', async () => {
  const { Outlet } = await import('react-router-dom')
  return { default: () => <Outlet /> }
})

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <App />
    </MemoryRouter>
  )
}

describe('app routes (#8 flatten + /vote aliases)', () => {
  it('serves the list at / (no client redirect to /vote)', () => {
    renderAt(ROUTES.list)
    expect(screen.getByTestId('list-page')).toBeVisible()
  })

  it('serves the list at the /vote alias', () => {
    renderAt(VOTE_PREFIX_ALIASES.list)
    expect(screen.getByTestId('list-page')).toBeVisible()
  })

  it('serves new-proposal at /new', () => {
    renderAt(ROUTES.newProposal)
    expect(screen.getByTestId('new-page')).toBeVisible()
  })

  it('serves new-proposal at /vote/new', () => {
    renderAt(VOTE_PREFIX_ALIASES.newProposal)
    expect(screen.getByTestId('new-page')).toBeVisible()
  })

  it('serves detail at /:id', () => {
    renderAt(ROUTES.proposal('11111111-1111-1111-1111-111111111111'))
    expect(screen.getByTestId('detail-page')).toBeVisible()
  })

  it('serves detail at /vote/:id', () => {
    renderAt(VOTE_PREFIX_ALIASES.proposal('11111111-1111-1111-1111-111111111111'))
    expect(screen.getByTestId('detail-page')).toBeVisible()
  })

  it('does not treat /new as a proposal id', () => {
    renderAt('/new')
    expect(screen.getByTestId('new-page')).toBeVisible()
    expect(screen.queryByTestId('detail-page')).not.toBeInTheDocument()
  })
})
