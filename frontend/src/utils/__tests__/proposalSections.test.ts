import { describe, expect, it } from 'vitest'
import {
  GOLDEN_BODY_HASH,
  GOLDEN_CANONICAL_JSON,
  GOLDEN_SECTIONS,
  MAX_SUMMARY_VISIBLE_CHARS,
  MIN_SECTION_VISIBLE_CHARS,
  canonicalizeSections,
  hashSections,
  plainToSectionHtml,
  sanitizeSectionHtml,
  sanitizeSectionObject,
  sectionsFromPlain,
  validateSections,
  visibleLen,
} from '@/utils/proposalSections'

describe('proposal sections', () => {
  it('counts visible characters after strip-tags and whitespace collapse', () => {
    expect(visibleLen('')).toBe(0)
    expect(visibleLen('<p></p>')).toBe(0)
    expect(visibleLen('<p><br></p>')).toBe(0)
    expect(visibleLen('<p> </p>')).toBe(0)
    expect(visibleLen('&nbsp;')).toBe(0)
    expect(visibleLen('<p>&#160;</p>')).toBe(0)
    expect(visibleLen('<p>\u200B\u200B</p>')).toBe(0)
    expect(visibleLen('<p>hello</p>')).toBe(5)
    expect(visibleLen('<p>  hello   world  </p>')).toBe(11)
  })

  it('matches the Rust golden canonical JSON and SHA-256', async () => {
    expect(canonicalizeSections(GOLDEN_SECTIONS)).toBe(GOLDEN_CANONICAL_JSON)
    expect(await hashSections(GOLDEN_SECTIONS)).toBe(GOLDEN_BODY_HASH)
  })

  it('hashes independently of object key insertion order', async () => {
    const shuffled = {
      summary: GOLDEN_SECTIONS.summary,
      success_criteria: GOLDEN_SECTIONS.success_criteria,
      pros_cons: GOLDEN_SECTIONS.pros_cons,
      solution: GOLDEN_SECTIONS.solution,
      problem: GOLDEN_SECTIONS.problem,
      context: GOLDEN_SECTIONS.context,
    }
    expect(canonicalizeSections(shuffled)).toBe(GOLDEN_CANONICAL_JSON)
    expect(await hashSections(shuffled)).toBe(GOLDEN_BODY_HASH)
  })

  it('does not treat empty HTML padding as a required section', () => {
    const base = sectionsFromPlain({
      context: '',
      problem: 'p'.repeat(MIN_SECTION_VISIBLE_CHARS),
      solution: 's'.repeat(MIN_SECTION_VISIBLE_CHARS),
      pros_cons: 't'.repeat(MIN_SECTION_VISIBLE_CHARS),
      summary: 'm'.repeat(MIN_SECTION_VISIBLE_CHARS),
      success_criteria: 'c'.repeat(MIN_SECTION_VISIBLE_CHARS),
    })
    expect(validateSections(base).ok).toBe(true)
    expect(validateSections({ ...base, context: '' }).ok).toBe(true)
    expect(validateSections({ ...base, pros_cons: sanitizeSectionHtml('<p></p>') }).ok).toBe(false)
    expect(validateSections({ ...base, summary: 's'.repeat(MAX_SUMMARY_VISIBLE_CHARS + 1) }).ok).toBe(
      false
    )
    expect(
      validateSections({
        ...base,
        summary: plainToSectionHtml('s'.repeat(MAX_SUMMARY_VISIBLE_CHARS)),
      }).ok
    ).toBe(true)
  })

  it('strips script and onerror before hashing', async () => {
    const dirty = sanitizeSectionObject({
      ...GOLDEN_SECTIONS,
      problem: `<p>${'x'.repeat(MIN_SECTION_VISIBLE_CHARS)}</p><script>alert(1)</script><img src=x onerror="alert(2)">`,
    })
    expect(dirty.problem).not.toMatch(/script|onerror|alert/i)
    expect(await hashSections(dirty)).not.toBe(GOLDEN_BODY_HASH)
  })
})
