import { expect, test, type Page } from '@playwright/test'

const ADDR = 'terra1testxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'
let registered = false
const proposals: Array<{
  id: string
  chain: string
  proposer: string
  title: string
  body_html: string
  terra_height: number
  bsc_block: number
  created_at: string
  status: string
  tally: Record<string, string>
  advisory: boolean
}> = []
const votes = new Map<string, string>()

async function mockOperatorVoting(page: Page) {
  await page.route('**/v1/proposals', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        json: proposals.map(({ body_html: _b, ...rest }) => rest),
      })
      return
    }
    const body = route.request().postDataJSON() as { title: string; body_html: string; address: string }
    const id = '11111111-1111-1111-1111-111111111111'
    proposals.push({
      id,
      chain: 'terra',
      proposer: body.address,
      title: body.title,
      body_html: body.body_html,
      terra_height: 1,
      bsc_block: 1,
      created_at: new Date().toISOString(),
      status: 'open',
      tally: {},
      advisory: true,
    })
    await route.fulfill({ status: 201, json: { id, terra_height: 1, bsc_block: 1 } })
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
    registered = true
    await route.fulfill({ json: { ok: true, pending: false } })
  })

  await page.route('**/v1/registration/**', async (route) => {
    await route.fulfill({
      status: registered ? 200 : 404,
      json: registered ? { terra: { chain: 'terra', wallet_address: ADDR, status: 'active' } } : { error: 'not registered' },
    })
  })

  await page.route('**/v1/balances/**', async (route) => {
    await route.fulfill({
      json: {
        chain: 'terra',
        address: ADDR,
        height: 1,
        balance: '1000000000000000000000',
      },
    })
  })
}

test('register → list → create → vote', async ({ page }) => {
  await mockOperatorVoting(page)
  await page.addInitScript(() => {
    localStorage.setItem('cl8y_dev_sim', '1')
    window.keplr = {
      enable: async () => undefined,
      experimentalSuggestChain: async () => undefined,
      getKey: async () => ({
        name: 'e2e',
        bech32Address: 'terra1testxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
        pubKey: new Uint8Array(33),
        isNanoLedger: false,
      }),
      getOfflineSigner: () => ({}),
      signArbitrary: async () => ({
        signature: 'c2lnbmF0dXJl',
        pub_key: { type: 'tendermint/PubKeySecp256k1', value: 'cHVia2V5' },
      }),
    }
  })

  await page.goto('/vote')
  await expect(page.getByRole('heading', { name: 'Proposals' })).toBeVisible()

  const connect = page.getByTestId('wallet-connect')
  if (await connect.isVisible()) {
    await connect.click()
    await page.getByTestId('connect-simulated-terra').click()
  }

  await expect(page.getByTestId('register-cta')).toBeVisible()
  await page.getByTestId('register-cta').click()
  await expect(page.getByTestId('registration-status')).toBeVisible({ timeout: 15_000 })

  await page.getByTestId('propose-cta').click()
  await page.getByTestId('proposal-title').fill('E2E proposal')
  await page.locator('.tiptap').fill('Body from Playwright')
  await page.getByTestId('submit-proposal').click()

  await expect(page.getByRole('heading', { name: 'E2E proposal' })).toBeVisible({ timeout: 15_000 })
  await page.getByTestId('vote-for').click()
  await expect(page.getByTestId('vote-recorded')).toContainText('for')
})
