import { type Page } from '@playwright/test'

export const E2E_TERRA_ADDR = 'terra1testxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'

export const E2E_PROPOSAL_ID = '11111111-1111-1111-1111-111111111111'

export type MockOperatorOpts = {
  registerPendingTicks?: number
  alreadyPending?: boolean
}

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
    body_sections: Record<string, string> | null
    summary: string | null
    terra_height: number
    bsc_block: number
    created_at: string
    status: string
    tally: Record<string, string>
    advisory: boolean
  }> = []
  const votes = new Map<string, string>()

  await page.route('**/v1/proposals', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        json: proposals.map(({ body_html: _b, body_sections: _s, ...rest }) => rest),
      })
      return
    }
    const body = route.request().postDataJSON() as {
      title: string
      body_html?: string
      body_sections?: Record<string, string>
      address: string
    }
    const sections = body.body_sections ?? null
    proposals.push({
      id: E2E_PROPOSAL_ID,
      chain: 'terra',
      proposer: body.address,
      title: body.title,
      body_html: body.body_html ?? '',
      body_sections: sections,
      summary: sections?.summary ?? null,
      terra_height: 1,
      bsc_block: 1,
      created_at: new Date().toISOString(),
      status: 'open',
      tally: {},
      advisory: true,
    })
    await route.fulfill({ status: 201, json: { id: E2E_PROPOSAL_ID, terra_height: 1, bsc_block: 1 } })
  })

  await page.route('**/v1/proposals/*/votes', async (route) => {
    const body = route.request().postDataJSON() as { choice: string; address: string }
    votes.set(body.address, body.choice)
    await route.fulfill({ json: { ok: true, weight: '1000000000000000000000' } })
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
    if (pendingTicks <= 0) ledgerReady = true
    await route.fulfill({ json: { ok: true, pending: !ledgerReady } })
  })

  await page.route('**/v1/registration/**', async (route) => {
    if (!registerPosted && !ledgerReady) {
      await route.fulfill({ status: 404, json: { error: 'not registered' } })
      return
    }
    if (!ledgerReady && pendingTicks > 0) {
      pendingTicks -= 1
      if (pendingTicks > 0) {
        await route.fulfill({
          status: 200,
          json: { terra: null, bsc: null, pending: { terra: true, bsc: false } },
        })
        return
      }
      ledgerReady = true
    }
    if (!ledgerReady) {
      await route.fulfill({ status: 404, json: { error: 'not registered' } })
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
