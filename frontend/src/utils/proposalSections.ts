/**
 * Structured proposal template (issues #10 / #11).
 * Canonical JSON must match operator-voting `sections.rs`: compact UTF-8,
 * sorted keys, ammonia-sanitized HTML values.
 */

import { sanitizeProposalHtml } from '@/utils/sanitizeProposalHtml'

export const MIN_SECTION_VISIBLE = 40
export const MAX_SUMMARY_VISIBLE = 500

export const CANONICAL_SECTION_KEYS = [
  'context',
  'problem',
  'pros_cons',
  'solution',
  'success_criteria',
  'summary',
] as const

export type SectionKey = (typeof CANONICAL_SECTION_KEYS)[number]

export const REQUIRED_SECTION_KEYS: SectionKey[] = [
  'problem',
  'solution',
  'pros_cons',
  'summary',
  'success_criteria',
]

export type ProposalSections = Record<SectionKey, string>

export const SECTION_LABELS: Record<SectionKey, string> = {
  problem: 'The idea or problem to be solved',
  context: 'Supporting context, real-world issues, and alternatives',
  solution: 'Proposed solution and supporting evidence',
  pros_cons: 'Pros and cons / trade-offs',
  summary: 'Summary (TL;DR)',
  success_criteria: 'Success criteria / goalposts',
}

export const SECTION_HELP: Record<SectionKey, string> = {
  problem: 'What should change, and for whom?',
  context: 'Optional. Alternatives and background.',
  solution: 'What you propose and the evidence behind it.',
  pros_cons: 'Trade-offs. Do not leave critique only as self-praise.',
  summary: 'Short enough for the proposal list. Max 500 visible characters.',
  success_criteria: 'Goalposts for later spend/outcome justification — not a punishment clause.',
}

export const CANONICAL_ANALYSIS_KEYS = ['benefits', 'long_term', 'risks', 'short_term', 'what'] as const
export type AnalysisKey = (typeof CANONICAL_ANALYSIS_KEYS)[number]
export type AnalysisSections = Record<AnalysisKey, string>

export const ANALYSIS_LABELS: Record<AnalysisKey, string> = {
  what: 'What is proposed',
  benefits: 'Benefits',
  risks: 'Risks',
  short_term: 'Short-term impact',
  long_term: 'Long-term impact',
}

export function emptySections(): ProposalSections {
  return {
    context: '',
    problem: '',
    pros_cons: '',
    solution: '',
    success_criteria: '',
    summary: '',
  }
}

export function emptyAnalysis(): AnalysisSections {
  return { benefits: '', long_term: '', risks: '', short_term: '', what: '' }
}

export function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

export function textToSectionHtml(raw: string): string {
  const t = raw.trim()
  if (!t) return ''
  return `<p>${escapeHtml(t)}</p>`
}

export function visibleText(html: string): string {
  const stripped = html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ')
    .replace(/[\u200b\u200c\u200d\ufeff]/g, '')
    .replace(/\u00a0/g, ' ')
  return stripped.split(/\s+/).filter(Boolean).join(' ')
}

export function visibleLen(html: string): number {
  return [...visibleText(html)].length
}

export function sanitizeSections(raw: ProposalSections): ProposalSections {
  const out = emptySections()
  for (const key of CANONICAL_SECTION_KEYS) {
    out[key] = sanitizeProposalHtml(raw[key] ?? '')
  }
  return out
}

export function sanitizeAnalysis(raw: AnalysisSections): AnalysisSections {
  const out = emptyAnalysis()
  for (const key of CANONICAL_ANALYSIS_KEYS) {
    out[key] = sanitizeProposalHtml(raw[key] ?? '')
  }
  return out
}

export function canonicalizeProposalSections(sections: ProposalSections): string {
  const ordered: Record<string, string> = {}
  for (const key of CANONICAL_SECTION_KEYS) {
    ordered[key] = sections[key] ?? ''
  }
  return JSON.stringify(ordered)
}

export function canonicalizeAnalysisSections(sections: AnalysisSections): string {
  const ordered: Record<string, string> = {}
  for (const key of CANONICAL_ANALYSIS_KEYS) {
    ordered[key] = sections[key] ?? ''
  }
  return JSON.stringify(ordered)
}

export function sectionErrors(sections: ProposalSections): Partial<Record<SectionKey, string>> {
  const errors: Partial<Record<SectionKey, string>> = {}
  for (const key of REQUIRED_SECTION_KEYS) {
    if (visibleLen(sections[key]) < MIN_SECTION_VISIBLE) {
      errors[key] = `Need at least ${MIN_SECTION_VISIBLE} visible characters`
    }
  }
  if (visibleLen(sections.summary) > MAX_SUMMARY_VISIBLE) {
    errors.summary = `Summary must be at most ${MAX_SUMMARY_VISIBLE} visible characters`
  }
  return errors
}

export function sectionsAreValid(sections: ProposalSections): boolean {
  return Object.keys(sectionErrors(sections)).length === 0
}

export function analysisErrors(sections: AnalysisSections): Partial<Record<AnalysisKey, string>> {
  const errors: Partial<Record<AnalysisKey, string>> = {}
  for (const key of CANONICAL_ANALYSIS_KEYS) {
    if (visibleLen(sections[key]) < MIN_SECTION_VISIBLE) {
      errors[key] = `Need at least ${MIN_SECTION_VISIBLE} visible characters`
    }
  }
  return errors
}
