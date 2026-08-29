/**
 * Voting dApp paths on the dedicated `vote.cl8y.com` host (issue #8).
 *
 * Canonical routes are `/`, `/new`, `/:id` so Legal portal full-navigation
 * after Accept lands on `/` when the user connected on the list page. `/`
 * already returns 200 HTML even on an edge that only serves `index.html` at
 * the origin root. `/vote*` aliases keep DEX-era bookmarks and in-flight
 * Legal `redirect_uri` values working.
 *
 * HTTP 200 + SPA `index.html` for every app path is still required. Aliases
 * without nginx `try_files` still 404 on hard navigation / Legal return.
 * See docs/FRONTEND.md, docs/OPS.md (O8), skills/AGENTS_LEGAL_CLICKWRAP.md.
 */
export const ROUTES = {
  list: '/',
  newProposal: '/new',
  proposal: (id: string) => `/${id}`,
} as const

export const VOTE_PREFIX_ALIASES = {
  list: '/vote',
  newProposal: '/vote/new',
  proposal: (id: string) => `/vote/${id}`,
} as const
