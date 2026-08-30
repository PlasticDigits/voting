import { expect, test, type Page } from '@playwright/test'
import { E2E_PROPOSAL_ID, installSimulatedKeplr, mockOperatorVoting } from './operatorMock'

async function connectSimulatedTerra(page: Page, path: string) {
  await installSimulatedKeplr(page)
  await page.goto(path)
  await expect(page.getByRole('heading', { name: 'Proposals' })).toBeVisible()
  const connect = page.getByTestId('wallet-connect')
  if (await connect.isVisible()) {
    await connect.click()
    await page.getByTestId('connect-simulated-terra').click()
  }
}

async function registerProposeVote(page: Page, path: string, title: string) {
  await mockOperatorVoting(page)
  await connectSimulatedTerra(page, path)

  await expect(page.getByTestId('register-cta')).toBeVisible()
  await page.getByTestId('register-cta').click()
  await expect(page.getByTestId('registration-status')).toBeVisible({ timeout: 15_000 })

  await page.getByTestId('propose-cta').click()
  await expect(page.getByTestId('propose-balance')).toContainText('1000 CL8Y')
  await page.getByTestId('proposal-title').fill(title)
  const fill = 'Playwright section text that clears the forty character minimum.'
  await page.getByTestId('proposal-section-problem').fill(fill)
  await page.getByTestId('proposal-section-solution').fill(fill)
  await page.getByTestId('proposal-section-pros_cons').fill(fill)
  await page.getByTestId('proposal-section-summary').fill(fill)
  await page.getByTestId('proposal-section-success_criteria').fill(fill)
  await page.getByTestId('submit-proposal').click()

  await expect(page.getByRole('heading', { name: title })).toBeVisible({ timeout: 15_000 })
  await expect(page.getByTestId('proposal-sections')).toBeVisible()
  await expect(page.getByTestId('proposal-section-view-summary')).toContainText('Summary (TL;DR)')
  await expect(page).toHaveURL(new RegExp(`/${E2E_PROPOSAL_ID}$`))
  await page.getByTestId('vote-for').click()
  await expect(page.getByTestId('vote-recorded')).toContainText('for')

  await page.getByRole('link', { name: 'All proposals' }).click()
  await expect(page.getByTestId('proposal-summary')).toContainText('Playwright section text')
}

test('unregistered connection does not look like a live 0 CL8Y', async ({ page }) => {
  await mockOperatorVoting(page)
  await connectSimulatedTerra(page, '/')
  await expect(page.getByTestId('register-cta')).toBeVisible()
  await expect(page.getByTestId('connected-chain')).not.toContainText('0 CL8Y')
  await expect(page.getByTestId('register-hint')).toBeVisible()
})

test('pending register polls until ledger-registered', async ({ page }) => {
  await mockOperatorVoting(page, { registerPendingTicks: 2 })
  await connectSimulatedTerra(page, '/')
  await page.getByTestId('register-cta').click()
  await expect(page.getByTestId('registration-pending')).toBeVisible()
  await expect(page.getByTestId('registration-status')).toBeVisible({ timeout: 15_000 })
  await expect(page.getByTestId('connected-chain')).toContainText('1000 CL8Y')
})

test('register → list → create → vote (canonical /)', async ({ page }) => {
  await registerProposeVote(page, '/', 'E2E proposal')
})

test('register → list → create → vote (/vote alias)', async ({ page }) => {
  await registerProposeVote(page, '/vote', 'E2E alias proposal')
})
