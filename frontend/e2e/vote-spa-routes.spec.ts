import { expect, test } from '@playwright/test'
import { E2E_PROPOSAL_ID, installSimulatedKeplr, mockOperatorVoting } from './operatorMock'

test('disconnected browse of / and /vote still opens the list', async ({ page }) => {
  await mockOperatorVoting(page)
  for (const path of ['/', '/vote']) {
    await page.goto(path)
    await expect(page.getByRole('heading', { name: 'Proposals' })).toBeVisible()
    await expect(page.getByTestId('wallet-connect')).toBeVisible()
    await expect(page.getByTestId('empty-proposals')).toBeVisible()
    await expect(page.getByTestId('connected-terms-gate')).toHaveCount(0)
  }
})

test('disconnected hard navigation of /new and /vote/new still opens (connect prompt)', async ({ page }) => {
  await mockOperatorVoting(page)
  for (const path of ['/new', '/vote/new']) {
    await page.goto(path)
    await expect(page.getByText('Connect a wallet to create a proposal.')).toBeVisible()
    await expect(page.getByRole('link', { name: 'Back to proposals' })).toBeVisible()
  }
})

test('hard navigation of /new and /vote/new with simulated wallet shows compose', async ({ page }) => {
  await mockOperatorVoting(page)
  await installSimulatedKeplr(page)
  for (const path of ['/new', '/vote/new']) {
    await page.goto(path, { waitUntil: 'domcontentloaded' })
    await expect(page.getByRole('heading', { name: 'New proposal' })).toBeVisible()
  }
})

test('hard navigation of /:id and /vote/:id loads the SPA (API 404 is in-app)', async ({ page }) => {
  await mockOperatorVoting(page)
  for (const path of [`/${E2E_PROPOSAL_ID}`, `/vote/${E2E_PROPOSAL_ID}`]) {
    await page.goto(path)
    await expect(page.locator('.app-shell')).toBeVisible()
    await expect(page.getByRole('alert')).toBeVisible()
  }
})

test('client navigation list → new → back stays on canonical paths', async ({ page }) => {
  await mockOperatorVoting(page)
  await installSimulatedKeplr(page)
  await page.goto('/')
  const connect = page.getByTestId('wallet-connect')
  if (await connect.isVisible()) {
    await connect.click()
    await page.getByTestId('connect-simulated-terra').click()
  }
  await page.getByTestId('propose-cta').click()
  await expect(page).toHaveURL(/\/new$/)
  await expect(page.getByRole('heading', { name: 'New proposal' })).toBeVisible()
  await page.getByTestId('brand-home').click()
  await expect(page).toHaveURL(/\/$/)
  await expect(page.getByRole('heading', { name: 'Proposals' })).toBeVisible()
})
