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

async function registerDraftCommentOpenVote(page: Page, path: string, title: string) {
  await mockOperatorVoting(page)
  await connectSimulatedTerra(page, path)

  await expect(page.getByTestId('register-cta')).toBeVisible()
  await page.getByTestId('register-cta').click()
  await expect(page.getByTestId('registration-status')).toBeVisible({ timeout: 15_000 })

  await page.getByTestId('propose-cta').click()
  await expect(page.getByTestId('propose-balance')).toContainText('1000 CL8Y')
  await page.getByTestId('proposal-title').fill(title)
  const section = 'This draft section has forty visible characters.'
  for (const key of ['problem', 'solution', 'pros_cons', 'summary', 'success_criteria']) {
    await page.getByTestId(`section-${key}`).fill(section)
  }
  await page.getByTestId('submit-proposal').click()

  await expect(page.getByRole('heading', { name: title })).toBeVisible({ timeout: 15_000 })
  await expect(page).toHaveURL(new RegExp(`/${E2E_PROPOSAL_ID}$`))
  await expect(page.getByTestId('proposal-status')).toHaveText('draft')
  await expect(page.getByTestId('vote-for')).toHaveCount(0)

  await page.getByTestId('comment-body').fill('A registered reviewer note.')
  await page.getByTestId('submit-comment').click()
  await expect(page.getByTestId('comment-list')).toContainText('A registered reviewer note')

  await page.getByTestId('open-poll').click()
  await expect(page.getByTestId('proposal-status')).toHaveText('open')
  await page.getByTestId('vote-for').click()
  await expect(page.getByTestId('vote-recorded')).toContainText('for')
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

test('register → draft → comment → open → vote (canonical /)', async ({ page }) => {
  await registerDraftCommentOpenVote(page, '/', 'E2E proposal')
})

test('register → draft → comment → open → vote (/vote alias)', async ({ page }) => {
  await registerDraftCommentOpenVote(page, '/vote', 'E2E alias proposal')
})
