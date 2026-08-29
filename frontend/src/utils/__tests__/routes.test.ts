import { describe, expect, it } from 'vitest'
import { ROUTES, VOTE_PREFIX_ALIASES } from '@/routes'

describe('route constants', () => {
  it('keeps canonical paths on the dedicated host and /vote aliases', () => {
    expect(ROUTES.list).toBe('/')
    expect(ROUTES.newProposal).toBe('/new')
    expect(ROUTES.proposal('abc')).toBe('/abc')
    expect(VOTE_PREFIX_ALIASES.list).toBe('/vote')
    expect(VOTE_PREFIX_ALIASES.newProposal).toBe('/vote/new')
    expect(VOTE_PREFIX_ALIASES.proposal('abc')).toBe('/vote/abc')
  })
})
