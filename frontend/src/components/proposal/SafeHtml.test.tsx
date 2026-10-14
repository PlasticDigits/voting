import { render } from '@testing-library/react'
import { describe, it, expect, vi } from 'vitest'
import SafeHtml from './SafeHtml'
import { ProposalSectionView } from './ProposalSectionFields'
import { sanitizeProposalHtml } from '@/utils/sanitizeProposalHtml'
const corpus = [
  '<div><a href="javascript:alert(1)" onclick="alert(2)">nested</a></div>',
  '<svg><g onload="alert(1)"></g></svg><math><mtext><img src=x onerror=alert(1)></mtext></math>',
  '<form id=attributes><input name=children></form><p id=__proto__ onclick=x()>text</p>',
  '<a href="data:text/html,<script>alert(1)</script>">data</a>',
  '<math><mtext><table><mglyph><style><!--</style><img title="--><img src=1 onerror=alert(1)>">',
]
function safe(container: HTMLElement) {
  expect(container.querySelector('script,svg,math,img,style,form,iframe')).toBeNull()
  for (const el of container.querySelectorAll('*'))
    for (const a of el.attributes) {
      expect(a.name).not.toMatch(/^on|^id$|^name$/)
      if (a.name === 'href') expect(a.value).not.toMatch(/^(javascript|data):/i)
    }
}
describe('HTML display boundary', () => {
  for (const attack of corpus)
    it(`neutralizes ${attack.slice(0, 35)}`, () => {
      const { container } = render(<SafeHtml html={attack} />)
      safe(container)
      const legacy = render(<ProposalSectionView fallbackHtml={attack} />)
      safe(legacy.container)
      const sections = render(<ProposalSectionView sections={{ summary: attack }} />)
      safe(sections.container)
    })
  it('bounds hostile oversized display inputs', () => {
    expect(sanitizeProposalHtml('<p>' + 'x'.repeat(256 * 1024) + '</p>')).toBe('')
  })
  it('fails closed without a DOM', () => {
    vi.stubGlobal('DOMParser', undefined)
    expect(sanitizeProposalHtml('<a href="javascript:x()">unsafe</a>')).toBe('')
    vi.unstubAllGlobals()
  })
})
