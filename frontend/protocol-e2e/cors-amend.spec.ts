import { test, expect, type Page } from '@playwright/test'
import { privateKeyToAccount } from 'viem/accounts'
import { createHash, randomUUID } from 'node:crypto'
// Public unit-test key, has no funds or authorization outside the isolated fixture DB.
const account = privateKeyToAccount(`0x${'11'.repeat(32)}`)
const origin = 'http://127.0.0.1:3991'
async function send(page: Page, path: string, body: unknown, method = 'POST') {
  return page.evaluate(
    async ({ origin, path, body, method }) => {
      const r = await fetch(origin + path, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
      return { status: r.status, json: await r.json(), requestId: r.headers.get('x-request-id') }
    },
    { origin, path, body, method },
  )
}
async function sign(purpose: string, fields: Record<string, unknown> = {}) {
  const now = Math.floor(Date.now() / 1000)
  const payload = JSON.stringify({
    version: 2,
    nonce: randomUUID(),
    app: 'cl8y-voting',
    chain: 'bsc',
    chain_id: '56',
    purpose,
    address: account.address,
    issued_at: now,
    expires_at: now + 180,
    ...fields,
  })
  return {
    chain: 'bsc',
    address: account.address,
    payload,
    signature: await account.signMessage({ message: payload }),
  }
}
test('real second-origin browser creates and amends a signed draft through deployed middleware', async ({
  page,
}) => {
  await page.goto('/')
  const section =
    '<p>This browser fixture provides enough detail for all of the proposal sections 🚀.</p>'
  const sections = {
    context: '',
    problem: section,
    pros_cons: section,
    solution: section,
    success_criteria: section,
    summary: section,
  }
  const canon = await send(page, '/v1/canonicalize', { kind: 'proposal', sections })
  expect(canon.status).toBe(200)
  expect(createHash('sha256').update(canon.json.canonical_json).digest('hex')).toBe(
    canon.json.body_hash,
  )
  const draft = await send(page, '/v1/proposals', {
    ...(await sign('draft', { title: 'Browser CORS amendment', body_hash: canon.json.body_hash })),
    title: 'Browser CORS amendment',
    body_sections: canon.json.sections,
  })
  expect(draft.status).toBe(201)
  expect(draft.json.revision).toBe(1)
  const amendedSections = { ...sections, problem: section.replace('browser', 'amended browser') }
  const next = await send(page, '/v1/canonicalize', { kind: 'proposal', sections: amendedSections })
  const signed = await sign('amend', {
    title: 'Browser CORS amendment',
    proposal_id: draft.json.id,
    body_hash: next.json.body_hash,
    prev_body_hash: canon.json.body_hash,
    revision: 1,
  })
  const amendment = await send(
    page,
    `/v1/proposals/${draft.json.id}/sections`,
    { ...signed, title: 'Browser CORS amendment', body_sections: next.json.sections },
    'PUT',
  )
  expect(amendment.status).toBe(200)
  expect(amendment.json.revision).toBe(2)
  expect(amendment.requestId).toBeTruthy()
  const retry = await send(
    page,
    `/v1/proposals/${draft.json.id}/sections`,
    { ...signed, title: 'Browser CORS amendment', body_sections: next.json.sections },
    'PUT',
  )
  expect(retry.status).toBe(200)
  expect(retry.json.receipt_id).toBe(amendment.json.receipt_id)
})
