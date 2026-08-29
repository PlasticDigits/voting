import { expect, test } from '@playwright/test'
import { installSimulatedKeplr, mockOperatorVoting } from './operatorMock'

test('register → list → create → vote (canonical /)', async ({ page }) => {
  await mockOperatorVoting(page)
  await installSimulatedKeplr(page)

  await page.goto('/')
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
  await expect(page).toHaveURL(/\/new$/)
  await page.getByTestId('proposal-title').fill('E2E proposal')
  await page.locator('.tiptap').fill('Body from Playwright')
  await page.getByTestId('submit-proposal').click()

  await expect(page.getByRole('heading', { name: 'E2E proposal' })).toBeVisible({ timeout: 15_000 })
  await expect(page).toHaveURL(/\/11111111-1111-1111-1111-111111111111$/)
  await page.getByTestId('vote-for').click()
  await expect(page.getByTestId('vote-recorded')).toContainText('for')
})

test('register → list → create → vote (/vote alias)', async ({ page }) => {
  await mockOperatorVoting(page)
  await installSimulatedKeplr(page)

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
  await page.getByTestId('proposal-title').fill('E2E alias proposal')
  await page.locator('.tiptap').fill('Body from Playwright alias')
  await page.getByTestId('submit-proposal').click()

  await expect(page.getByRole('heading', { name: 'E2E alias proposal' })).toBeVisible({ timeout: 15_000 })
  await page.getByTestId('vote-for').click()
  await expect(page.getByTestId('vote-recorded')).toContainText('for')
})
