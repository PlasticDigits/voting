import { describe, expect, it } from 'vitest'
import {
  canonicalizeProposalSections,
  MIN_SECTION_VISIBLE,
  sanitizeSections,
  sectionErrors,
  visibleLen,
} from '@/utils/proposalSections'

const sample = {
  context: '',
  problem: '<p>problem needs forty visible characters in this fixture text.</p>',
  pros_cons: '<p>tradeoffs needs forty visible characters in this fixture text.</p>',
  solution: '<p>solution needs forty visible characters in this fixture text.</p>',
  success_criteria: '<p>success needs forty visible characters in this fixture text.</p>',
  summary: '<p>summary needs forty visible characters in this fixture text.</p>',
}

describe('proposal sections', () => {
  it('canonical JSON uses sorted keys and compact encoding', () => {
    const canonical = canonicalizeProposalSections(sanitizeSections(sample))
    expect(canonical.startsWith('{"context":""')).toBe(true)
    expect(canonical).toContain('"problem":')
    expect(canonical).toContain('"pros_cons":')
    expect(canonical).toContain('"solution":')
    expect(canonical).toContain('"success_criteria":')
    expect(canonical).toContain('"summary":')
    expect(canonicalizeProposalSections(sample)).toBe(canonical)
  })

  it('rejects empty required sections including HTML-only padding', () => {
    expect(visibleLen('<p></p>')).toBeLessThan(MIN_SECTION_VISIBLE)
    expect(visibleLen('<p>&nbsp;&nbsp;</p>')).toBeLessThan(MIN_SECTION_VISIBLE)
    const errors = sectionErrors({ ...sample, problem: '<p>   </p>' })
    expect(errors.problem).toBeTruthy()
  })
})
