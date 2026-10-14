import { createHash } from 'node:crypto'
import { type Page } from '@playwright/test'

export const E2E_TERRA_ADDR = 'terra1testxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'

export const E2E_PROPOSAL_ID = '11111111-1111-1111-1111-111111111111'

export type MockOperatorOpts = {
  registerPendingTicks?: number
  alreadyPending?: boolean
  /** Intent exists but `voting_registrations` never lands (issue #14 timeout/retry). */
  snapshotNeverReady?: boolean
}

const SECTION_HTML = '<p>This draft section has forty visible characters for the template.</p>'

export async function mockOperatorVoting(page: Page, opts: MockOperatorOpts = {}) {
  let ledgerReady = false
  let registerPosted = Boolean(opts.alreadyPending)
  let pendingTicks = opts.registerPendingTicks ?? 0
  const proposals: Array<{
    id: string
    chain: string
    proposer: string
    title: string
    body_html: string
    summary: string
    terra_height: number | null
    bsc_block: number | null
    created_at: string
    opened_at: string | null
    status: string
    tally: Record<string, string>
    advisory: boolean
    body_sections: Record<string, string>
    comments: Array<{
      id: string
      chain: string
      wallet_address: string
      body_html: string
      created_at: string
    }>
    analysis: unknown[]
  }> = []
  const votes = new Map<string, string>()
  await page.route('**/v1/metadata', async (route) =>
    route.fulfill({
      json: {
        protocol_version: 2,
        min_proposal_raw: '1000000000000000000000',
        decimals: 18,
        capabilities: ['canonicalize'],
      },
    }),
  )
  await page.route('**/v1/canonicalize', async (route) => {
    const body = route.request().postDataJSON() as {
      kind: string
      sections?: Record<string, string>
      body_html?: string
    }
    const sections = body.sections
      ? Object.fromEntries(
          Object.keys(body.sections)
            .sort()
            .map((key) => [key, body.sections![key]]),
        )
      : null
    const canonical_json = sections ? JSON.stringify(sections) : null
    const body_html = body.body_html ?? ''
    await route.fulfill({
      json: {
        protocol_version: 2,
        sections,
        canonical_json,
        body_html,
        body_hash: createHash('sha256')
          .update(canonical_json ?? body_html)
          .digest('hex'),
      },
    })
  })

  await page.route('**/v1/proposals', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        json: proposals.map(
          ({ body_html: _b, body_sections: _s, comments: _c, analysis: _a, ...rest }) => rest,
        ),
      })
      return
    }
    const body = route.request().postDataJSON() as {
      title: string
      body_sections?: Record<string, string>
      address: string
    }
    proposals.push({
      id: E2E_PROPOSAL_ID,
      revision: 1,
      snapshot_validity: 'valid',
      snapshot_incident: null,
      body_hash: createHash('sha256')
        .update(JSON.stringify(body.body_sections ?? {}))
        .digest('hex'),
      chain: 'terra',
      proposer: body.address,
      title: body.title,
      body_html: body.body_sections?.problem ?? SECTION_HTML,
      summary: body.body_sections?.summary ?? SECTION_HTML,
      terra_height: null,
      bsc_block: null,
      created_at: new Date().toISOString(),
      opened_at: null,
      status: 'draft',
      tally: [],
      advisory: true,
      body_sections: body.body_sections ?? {
        context: '',
        problem: SECTION_HTML,
        solution: SECTION_HTML,
        pros_cons: SECTION_HTML,
        summary: SECTION_HTML,
        success_criteria: SECTION_HTML,
      },
      comments: [],
      analysis: [],
    })
    await route.fulfill({
      status: 201,
      json: { id: E2E_PROPOSAL_ID, status: 'draft', terra_height: null, bsc_block: null },
    })
  })

  await page.route('**/v1/proposals/*/weights/*', async (route) => {
    const url = new URL(route.request().url())
    const id = url.pathname.split('/v1/proposals/')[1]?.split('/')[0]
    const address = decodeURIComponent(url.pathname.split('/weights/')[1] ?? '')
    await route.fulfill({
      json: {
        proposal_id: id,
        address,
        chain: url.searchParams.get('chain') ?? 'terra',
        weight: '1000000000000000000000',
        eligible: true,
      },
    })
  })
  await page.route('**/v1/proposals/*/votes', async (route) => {
    const body = route.request().postDataJSON() as { choice: string; address: string }
    votes.set(body.address, body.choice)
    await route.fulfill({ json: { ok: true, weight: '1000000000000000000000' } })
  })

  await page.route('**/v1/proposals/*/votes/*', async (route) => {
    const url = new URL(route.request().url())
    const address = decodeURIComponent(url.pathname.split('/votes/')[1] ?? '')
    const choice = votes.get(address)
    await route.fulfill(
      choice
        ? { json: { choice, weight: '1000000000000000000000' } }
        : { status: 404, json: { error: 'vote not found', code: 'not_found' } },
    )
  })
  await page.route('**/v1/proposals/*/comments', async (route) => {
    const id = route.request().url().split('/v1/proposals/')[1]?.split('/')[0]
    const found = proposals.find((p) => p.id === id)
    const body = route.request().postDataJSON() as { body_html: string; address: string }
    found?.comments.push({
      id: 'c1',
      chain: 'terra',
      wallet_address: body.address,
      body_html: body.body_html,
      created_at: new Date().toISOString(),
    })
    await route.fulfill({ status: 201, json: { id: 'c1' } })
  })

  await page.route('**/v1/proposals/*/open', async (route) => {
    const id = route.request().url().split('/v1/proposals/')[1]?.split('/')[0]
    const found = proposals.find((p) => p.id === id)
    if (found) {
      found.status = 'open'
      found.terra_height = 40
      found.bsc_block = 1
      found.opened_at = new Date().toISOString()
    }
    await route.fulfill({ json: { ok: true, status: 'open', terra_height: 40, bsc_block: 1 } })
  })

  await page.route('**/v1/proposals/*', async (route) => {
    const id = route.request().url().split('/v1/proposals/')[1]?.split('/')[0]
    const found = proposals.find((p) => p.id === id)
    if (!found) {
      await route.fulfill({ status: 404, json: { error: 'not found' } })
      return
    }
    await route.fulfill({ json: found })
  })

  await page.route('**/v1/register', async (route) => {
    registerPosted = true
    if (!opts.snapshotNeverReady && pendingTicks <= 0) ledgerReady = true
    await route.fulfill({ json: { ok: true, pending: !ledgerReady } })
  })

  await page.route('**/v1/registration/**', async (route) => {
    if (!registerPosted && !ledgerReady) {
      await route.fulfill({ status: 404, json: { error: 'not registered' } })
      return
    }
    const pendingBody = {
      terra: null,
      bsc: null,
      pending: { terra: true, bsc: false },
    }
    // GET 404s only when there is neither a ledger row nor an intent (L11 / OV-B3).
    // An in-flight intent must stay `pending`, not look unregistered.
    if (opts.snapshotNeverReady) {
      await route.fulfill({ status: 200, json: pendingBody })
      return
    }
    if (!ledgerReady && pendingTicks > 0) {
      pendingTicks -= 1
      if (pendingTicks > 0) {
        await route.fulfill({ status: 200, json: pendingBody })
        return
      }
      ledgerReady = true
    }
    if (!ledgerReady) {
      await route.fulfill({ status: 200, json: pendingBody })
      return
    }
    await route.fulfill({
      json: {
        terra: {
          chain: 'terra',
          wallet_address: E2E_TERRA_ADDR,
          registered_at_height: 10,
          status: 'active',
        },
        bsc: null,
        pending: { terra: false, bsc: false },
      },
    })
  })

  await page.route('**/v1/balances/**', async (route) => {
    await route.fulfill({
      json: {
        chain: 'terra',
        address: E2E_TERRA_ADDR,
        height: ledgerReady ? 10 : 0,
        as_of_height: ledgerReady ? 10 : 0,
        balance: ledgerReady ? '1000000000000000000000' : '0',
        registered: ledgerReady,
        snapshot_eligible: ledgerReady,
        pending: registerPosted && !ledgerReady,
        initial_balance: ledgerReady ? '1000000000000000000000' : null,
      },
    })
  })
}

export async function installSimulatedKeplr(page: Page) {
  await page.addInitScript((addr: string) => {
    localStorage.setItem('cl8y_dev_sim', '1')
    window.keplr = {
      enable: async () => undefined,
      experimentalSuggestChain: async () => undefined,
      getKey: async () => ({
        name: 'e2e',
        bech32Address: addr,
        pubKey: new Uint8Array(33),
        isNanoLedger: false,
      }),
      getOfflineSigner: () => ({}),
      signArbitrary: async () => ({
        signature: 'c2lnbmF0dXJl',
        pub_key: { type: 'tendermint/PubKeySecp256k1', value: 'cHVia2V5' },
      }),
    }
  }, E2E_TERRA_ADDR)
}
