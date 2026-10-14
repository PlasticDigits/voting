import { expect, test, type Locator, type Page } from '@playwright/test'
import {
  E2E_PROPOSAL_ID,
  E2E_TERRA_ADDR,
  installSimulatedKeplr,
  mockOperatorVoting,
} from './operatorMock'

// Deliberately hostile imported API data, not a claim that current sanitized writes store XSS.
const sections = ['context', 'problem', 'solution', 'pros_cons', 'summary', 'success_criteria'] as const
const analysis = ['benefits', 'long_term', 'risks', 'short_term', 'what'] as const
const effects = 'window.__votingXss++'
const corpus = [
  {
    name: 'nested disallowed wrappers retain no active descendants',
    html: `<div><section><p onclick="${effects}">click</p><a href="javascript:${effects}" onmouseover="${effects}">nested link</a></section></div>`,
  },
  {
    name: 'encoded and control-character URL schemes are rejected',
    html: `<a href="jav&#x61;script:${effects}">encoded</a><a href="java&#x09;script:${effects}">tab</a><a href="data:text/html,<script>${effects}</script>">data</a><a href="vbscript:msgbox(1)">vbscript</a><a href="file:///__voting_xss">file</a>`,
  },
  {
    name: 'SVG and MathML descendants cannot execute',
    html: `<svg onload="${effects}"><g><a xlink:href="javascript:${effects}">svg link</a></g></svg><math><mtext><img src="/__voting_xss_image" onerror="${effects}"></mtext></math>`,
  },
  {
    name: 'mutation XSS through malformed MathML nesting is neutralized',
    html: `<math><mtext><table><mglyph><style><!--</style><img title="--><img src='/__voting_xss_mutation' onerror='${effects}'>">`,
  },
  {
    name: 'forms and named properties cannot clobber the DOM',
    html: `<form id="__votingClobber" name="__votingClobber"><input name="attributes"><input name="children"></form><p id="__votingClobber" name="__proto__" onclick="${effects}">named paragraph</p><a id="__votingClobber" name="location">named anchor</a>`,
  },
  {
    name: 'script frames objects and CSS produce no effects or requests',
    html: `<script>${effects};fetch('https://xss.invalid/script')</script><iframe src="https://xss.invalid/frame" srcdoc="<script>parent.__votingXss++</script>"></iframe><object data="https://xss.invalid/object"></object><style>@import 'https://xss.invalid/style';</style><img src="https://xss.invalid/image" onerror="${effects}">`,
  },
  {
    name: 'malformed HTML and unsafe link descendants remain inert',
    html: `<div><p><a href="javascript:${effects}" onclick="${effects}"><b>link</div><img src="/__voting_xss_malformed" onerror="${effects}"></p><a href="data:text/html,unsafe">data tail`,
  },
  {
    name: 'out-of-range entities and Unicode do not bypass rendering',
    html: `<p>&#1114112; &#x110000; &#55296; &#xD800; &#0; 🚀 é 한국어</p><a href="&#106;avascript:${effects}" onfocus="${effects}">entity link</a>`,
  },
]

function proposal(html: string, structured: boolean) {
  return {
    id: E2E_PROPOSAL_ID,
    chain: 'terra',
    proposer: E2E_TERRA_ADDR,
    title: 'Hostile imported display fixture',
    revision: 1,
    body_hash: 'a'.repeat(64),
    status: 'draft',
    snapshot_validity: 'valid',
    snapshot_incident: null,
    terra_height: null,
    bsc_block: null,
    created_at: '2026-10-14T00:00:00Z',
    opened_at: null,
    advisory: true,
    summary: 'A harmless plain-text summary for this display fixture.',
    tally: [],
    body_html: html,
    body_sections: structured ? Object.fromEntries(sections.map((key) => [key, html])) : null,
    comments: [{
      id: 'comment',
      chain: 'terra',
      wallet_address: E2E_TERRA_ADDR,
      body_html: html,
      created_at: '2026-10-14T00:00:00Z',
    }],
    analysis: [{
      id: 'analysis',
      chain: 'terra',
      wallet_address: E2E_TERRA_ADDR,
      revision: 1,
      source: 'committee',
      machine_generated: false,
      body_html: html,
      sections: Object.fromEntries(analysis.map((key) => [key, html])),
      created_at: '2026-10-14T00:00:00Z',
    }],
  }
}

async function neutralized(surface: Locator) {
  await expect(surface).toBeVisible()
  const violations = await surface.evaluate((root) => {
    const forbidden = root.querySelectorAll(
      'script,style,svg,math,img,iframe,frame,object,embed,form,input,textarea,button,link,meta,base,template',
    )
    const violations: string[] = forbidden.length ? ['active element'] : []
    for (const element of root.querySelectorAll('*')) {
      for (const attribute of element.attributes) {
        if (/^(on|id$|name$|src$|srcdoc$|style$|formaction$|xlink:href$)/i.test(attribute.name))
          violations.push('active or named attribute')
        if (attribute.name === 'href') {
          const scheme = attribute.value.replace(/[\u0000-\u0020\u007f]/g, '')
          if (/^(javascript|data|vbscript|file|blob):/i.test(scheme))
            violations.push('unsafe URL scheme')
        }
      }
    }
    return violations
  })
  expect(violations).toEqual([])
  // Exercise surviving nodes after DOM insertion, including browser URL activation.
  await surface.locator('p').first().dispatchEvent('click')
  for (const anchor of await surface.locator('a').all()) {
    await anchor.dispatchEvent('mouseover')
    await anchor.evaluate((element: HTMLAnchorElement) => element.click())
  }
}

async function installEffectChecks(page: Page) {
  const events = {
    attackRequests: [] as string[],
    dialogs: [] as string[],
    popups: 0,
    pageErrors: [] as string[],
    mutations: [] as string[],
  }
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__votingXss = 0
  })
  page.on('dialog', async (dialog) => {
    events.dialogs.push(dialog.type())
    await dialog.dismiss()
  })
  page.on('popup', async (popup) => {
    events.popups++
    await popup.close()
  })
  page.on('pageerror', () => events.pageErrors.push('browser exception'))
  page.on('request', (request) => {
    const url = new URL(request.url())
    if (url.hostname === 'xss.invalid' || url.pathname.startsWith('/__voting_xss'))
      events.attackRequests.push('attack request')
    if (request.method() !== 'GET' && url.pathname.startsWith('/v1/'))
      events.mutations.push('API mutation')
  })
  await page.route('https://xss.invalid/**', (route) => route.abort())
  await page.route('**/__voting_xss*', (route) => route.abort())
  return async (expectedPath: string) => {
    // Allow deferred browser mutation/load effects two rendering turns to surface.
    await page.evaluate(() => new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    ))
    const state = await page.evaluate(() => ({
      effects: (window as unknown as Record<string, unknown>).__votingXss,
      clobbered: '__votingClobber' in window || document.getElementById('__votingClobber') !== null,
    }))
    expect(state).toEqual({ effects: 0, clobbered: false })
    expect(events).toEqual({
      attackRequests: [], dialogs: [], popups: 0, pageErrors: [], mutations: [],
    })
    expect(page.frames()).toHaveLength(1)
    expect(new URL(page.url()).pathname).toBe(expectedPath)
    expect(new URL(page.url()).hash).toBe('')
  }
}

for (const vector of corpus) {
  test(`Chromium HTML boundary: ${vector.name}`, async ({ page }) => {
    await mockOperatorVoting(page)
    const assertNoEffects = await installEffectChecks(page)
    // Prefix guarantees visible content even when a malicious fragment is entirely removed.
    const html = '<p>Readable proposal content remains available for every display boundary.</p>' + vector.html
    let structured = false
    await page.route(`**/v1/proposals/${E2E_PROPOSAL_ID}`, (route) =>
      route.fulfill({ json: proposal(html, structured) }),
    )
    for (const mode of ['legacy', 'structured'] as const) {
      structured = mode === 'structured'
      await page.goto(`/${E2E_PROPOSAL_ID}`)
      await expect(page.getByRole('heading', { name: 'Hostile imported display fixture' })).toBeVisible()
      if (structured) {
        for (const key of sections) await neutralized(page.getByTestId(`proposal-section-view-${key}`))
      } else {
        await neutralized(page.getByTestId('proposal-legacy-body'))
      }
      await neutralized(page.getByTestId('comment-list'))
      // Every independent analysis field reaches its production SafeHtml display.
      const fields = page.getByTestId('analysis-item').locator('.proposal-body')
      await expect(fields).toHaveCount(analysis.length)
      for (const field of await fields.all()) await neutralized(field)
      await assertNoEffects(`/${E2E_PROPOSAL_ID}`)
    }
    // Real compose preview treats user input as plain text; no signature or write is requested.
    await installSimulatedKeplr(page)
    await page.goto('/new')
    await expect(page.getByRole('heading', { name: 'New draft' })).toBeVisible()
    for (const key of sections) await page.getByTestId(`proposal-section-${key}`).fill(html)
    const preview = page.getByTestId('proposal-preview')
    for (const key of sections) {
      const field = preview.getByTestId(`proposal-section-view-${key}`)
      await neutralized(field)
      await expect(field).toContainText(vector.html)
    }
    await assertNoEffects('/new')
  })
}

test('Chromium preserves safe proposal formatting and HTTPS/mailto links', async ({ page }) => {
  await mockOperatorVoting(page)
  const html = '<p>Safe readable content <strong>bold</strong> 🚀 é <a href="https://example.com/docs">HTTPS</a> <a href="mailto:reader@example.invalid">Email</a></p>'
  await page.route(`**/v1/proposals/${E2E_PROPOSAL_ID}`, (route) =>
    route.fulfill({ json: proposal(html, true) }),
  )
  await page.goto(`/${E2E_PROPOSAL_ID}`)
  const summary = page.getByTestId('proposal-section-view-summary')
  await expect(summary.locator('strong')).toHaveText('bold')
  await expect(summary.locator('a').nth(0)).toHaveAttribute('href', 'https://example.com/docs')
  await expect(summary.locator('a').nth(1)).toHaveAttribute('href', 'mailto:reader@example.invalid')
  await expect(summary).toContainText('🚀 é')
})
