/**
 * Structured proposal sections (issue #10).
 *
 * Must stay aligned with `operator-voting/src/proposal_sections.rs` and
 * `docs/OPERATOR_VOTING.md` (OV-S1–S8). `body_hash` is SHA-256 of this
 * canonical JSON (sorted keys, ammonia-cleaned HTML values, compact encoding).
 */

import { sanitizeProposalHtml, sha256Hex } from './sanitizeProposalHtml'

export const SECTION_KEYS_SORTED = [
  'context',
  'problem',
  'pros_cons',
  'solution',
  'success_criteria',
  'summary',
] as const

export const SECTION_DISPLAY_ORDER = [
  'summary',
  'problem',
  'context',
  'solution',
  'pros_cons',
  'success_criteria',
] as const

export const REQUIRED_SECTION_KEYS = [
  'problem',
  'solution',
  'pros_cons',
  'summary',
  'success_criteria',
] as const

export const MIN_SECTION_VISIBLE_CHARS = 40
export const MAX_SUMMARY_VISIBLE_CHARS = 500
export const MAX_BODY_BYTES = 64 * 1024

export type SectionKey = (typeof SECTION_KEYS_SORTED)[number]
export type RequiredSectionKey = (typeof REQUIRED_SECTION_KEYS)[number]

export type ProposalSections = Record<SectionKey, string>

export const SECTION_LABELS: Record<SectionKey, string> = {
  summary: 'Summary (TL;DR)',
  problem: 'The idea or problem to be solved',
  context: 'Supporting context, real-world issues, and alternatives',
  solution: 'Proposed solution and supporting evidence',
  pros_cons: 'Pros and cons / trade-offs',
  success_criteria: 'Success criteria / goalposts',
}

export const SECTION_HELP: Record<SectionKey, string> = {
  summary: 'Short TL;DR shown on the list and at the top of the proposal.',
  problem: 'What is being asked, and why it matters.',
  context: 'Optional. Background, alternatives, or real-world constraints.',
  solution: 'What you propose, with the evidence you have.',
  pros_cons: 'Trade-offs. Name downsides; do not only list praise.',
  success_criteria:
    'Goalposts for spend and outcome justification — not a later punishment or slashing mechanism.',
}

export const EMPTY_SECTIONS: ProposalSections = {
  context: '',
  problem: '',
  pros_cons: '',
  solution: '',
  success_criteria: '',
  summary: '',
}

const ZERO_WIDTH = /[\u00AD\u200B\u200C\u200D\u2060\uFEFF]/g

export function stripTags(html: string): string {
  return html.replace(/<[^>]*>/g, '')
}

function decodeEntities(raw: string): string {
  return raw
    .replace(/&nbsp;/gi, '\u00A0')
    .replace(/&#x([0-9a-f]+);/gi, (_, hex: string) => {
      const cp = Number.parseInt(hex, 16)
      return Number.isFinite(cp) && cp >= 0 ? String.fromCodePoint(cp) : ''
    })
    .replace(/&#(\d+);/g, (_, dec: string) => {
      const cp = Number.parseInt(dec, 10)
      return Number.isFinite(cp) && cp >= 0 ? String.fromCodePoint(cp) : ''
    })
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
}

export function visibleText(html: string): string {
  const decoded = decodeEntities(stripTags(html)).replace(ZERO_WIDTH, '')
  return decoded.replace(/\s+/g, ' ').trim()
}

export function visibleLen(html: string): number {
  return [...visibleText(html)].length
}

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** Collapse whitespace then wrap in a single paragraph. Empty stays empty. */
export function plainToSectionHtml(text: string): string {
  const collapsed = text.replace(/\s+/g, ' ').trim()
  if (!collapsed) return ''
  return `<p>${escapeHtml(collapsed)}</p>`
}

export function emptySections(): ProposalSections {
  return { ...EMPTY_SECTIONS }
}

export function sanitizeSectionHtml(raw: string): string {
  const clean = sanitizeProposalHtml(raw)
  return visibleLen(clean) === 0 ? '' : clean
}

export function sanitizeSectionObject(input: Partial<ProposalSections> | Record<string, string>): ProposalSections {
  const out = emptySections()
  for (const key of SECTION_KEYS_SORTED) {
    out[key] = sanitizeSectionHtml(input[key] ?? '')
  }
  return out
}

export function canonicalizeSections(sections: ProposalSections): string {
  const ordered: Record<string, string> = {}
  for (const key of SECTION_KEYS_SORTED) {
    ordered[key] = sections[key]
  }
  return JSON.stringify(ordered)
}

export async function hashSections(sections: ProposalSections): Promise<string> {
  return sha256Hex(canonicalizeSections(sections))
}

export type SectionValidation = {
  ok: boolean
  errors: Partial<Record<SectionKey, string>>
}

export function validateSections(sections: ProposalSections): SectionValidation {
  const errors: Partial<Record<SectionKey, string>> = {}
  let combined = 0
  for (const key of SECTION_KEYS_SORTED) {
    combined += sections[key].length
  }
  if (combined > MAX_BODY_BYTES || canonicalizeSections(sections).length > MAX_BODY_BYTES) {
    errors.problem = 'Proposal is too large'
    return { ok: false, errors }
  }
  for (const key of REQUIRED_SECTION_KEYS) {
    const n = visibleLen(sections[key])
    if (n < MIN_SECTION_VISIBLE_CHARS) {
      errors[key] = `Need at least ${MIN_SECTION_VISIBLE_CHARS} characters`
    } else if (key === 'summary' && n > MAX_SUMMARY_VISIBLE_CHARS) {
      errors[key] = `Keep the summary to ${MAX_SUMMARY_VISIBLE_CHARS} characters`
    }
  }
  return { ok: Object.keys(errors).length === 0, errors }
}

export function sectionsFromPlain(plain: Record<SectionKey, string>): ProposalSections {
  const out = emptySections()
  for (const key of SECTION_KEYS_SORTED) {
    out[key] = sanitizeSectionHtml(plainToSectionHtml(plain[key] ?? ''))
  }
  return out
}

export function isSectionRecord(value: unknown): value is ProposalSections {
  if (!value || typeof value !== 'object') return false
  const rec = value as Record<string, unknown>
  return SECTION_KEYS_SORTED.every((key) => typeof rec[key] === 'string')
}

/** Golden vector shared with operator-voting `golden_canonical_json_and_hash`. */
export const GOLDEN_SECTIONS: ProposalSections = {
  context: '',
  problem: '<p>Holders often vote without enough information about impact and risks.</p>',
  pros_cons: '<p>Pros: proposals are comparable. Cons: writing takes more care.</p>',
  solution: '<p>Require labeled sections with server-enforced minimums.</p>',
  success_criteria: '<p>Voters can scan TL;DR, trade-offs, and goalposts before they sign.</p>',
  summary: '<p>Standard sections so every proposal is scannable before a vote.</p>',
}

export const GOLDEN_CANONICAL_JSON =
  '{"context":"","problem":"<p>Holders often vote without enough information about impact and risks.</p>","pros_cons":"<p>Pros: proposals are comparable. Cons: writing takes more care.</p>","solution":"<p>Require labeled sections with server-enforced minimums.</p>","success_criteria":"<p>Voters can scan TL;DR, trade-offs, and goalposts before they sign.</p>","summary":"<p>Standard sections so every proposal is scannable before a vote.</p>"}'

export const GOLDEN_BODY_HASH = '85b0e3985805be8d192178665cf0b745ddcb14cb5ccfc37d2b49c137fe0f5ed1'
