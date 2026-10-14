import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { Buffer } from 'node:buffer'
import { describe, it, expect } from 'vitest'
import { hashMessage } from 'viem'
import { canonicalizePayload, type SignedPayload } from '@/utils/votingPayload'
import { sha256Hex } from '@/utils/sanitizeProposalHtml'
const fixture = JSON.parse(
  readFileSync(resolve(__dirname, '../../../../protocol/fixtures/payload-v2.json'), 'utf8'),
) as {
  vectors: {
    payload: SignedPayload
    canonical_json: string
    eip191_hash: string
    adr36_doc: string
    adr36_sha256: string
  }[]
}
describe('shared v2 signing wire vectors', () => {
  for (const vector of fixture.vectors)
    it(`${vector.payload.chain} ${vector.payload.purpose} preserves exact wire and signing digests`, async () => {
      const canonical = canonicalizePayload(vector.payload)
      expect(canonical).toBe(vector.canonical_json)
      expect(hashMessage(canonical).slice(2)).toBe(vector.eip191_hash)
      const amino = JSON.stringify({
        account_number: '0',
        chain_id: '',
        fee: { amount: [], gas: '0' },
        memo: '',
        msgs: [
          {
            type: 'sign/MsgSignData',
            value: {
              data: Buffer.from(canonical, 'utf8').toString('base64'),
              signer: vector.payload.address,
            },
          },
        ],
        sequence: '0',
      })
      expect(amino).toBe(vector.adr36_doc)
      expect(await sha256Hex(amino)).toBe(vector.adr36_sha256)
    })
})
