import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, it, expect } from 'vitest'
import { sha256Hex } from '@/utils/sanitizeProposalHtml'
import { canonicalizeSections, visibleLen, type ProposalSections } from '@/utils/proposalSections'
const fixture = JSON.parse(
  readFileSync(resolve(__dirname, '../../../../protocol/fixtures/canonical-v2.json'), 'utf8'),
) as {
  vectors: {
    sections: ProposalSections
    canonical_json: string
    body_hash: string
    visible_length: number
  }[]
}
describe('shared server canonical vectors', () => {
  for (const [index, vector] of fixture.vectors.entries())
    it(`agrees on UTF8 JSON, hash and visible scalars ${index}`, async () => {
      expect(canonicalizeSections(vector.sections)).toBe(vector.canonical_json)
      expect(await sha256Hex(vector.canonical_json)).toBe(vector.body_hash)
      expect(visibleLen(vector.sections.summary)).toBe(vector.visible_length)
    })
})
