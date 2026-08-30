import { useMemo, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { useVotingSnapshot } from '@/hooks/useVotingSnapshot'
import { createProposal } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import { canPropose } from '@/utils/votingPayload'
import { MIN_PROPOSAL_CL8Y, MIN_PROPOSAL_RAW } from '@/utils/constants'
import { formatCl8y } from '@/utils/format'
import { ROUTES } from '@/routes'
import {
  type SectionKey,
  emptySections,
  hashSections,
  sectionsFromPlain,
  validateSections,
} from '@/utils/proposalSections'
import ProposalSectionFields, {
  ProposalSectionsPreview,
} from '@/components/proposal/ProposalSectionFields'

export default function VoteNewPage() {
  const { address, chain, label } = useConnectedIdentity()
  const snapshot = useVotingSnapshot(address, chain)
  const navigate = useNavigate()
  const [title, setTitle] = useState('')
  const [plain, setPlain] = useState<Record<SectionKey, string>>(emptySections)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const sections = useMemo(() => sectionsFromPlain(plain), [plain])
  const sectionCheck = useMemo(() => validateSections(sections), [sections])

  const registered = snapshot.status === 'registered'
  const gated = !registered || snapshot.balance == null || !canPropose(snapshot.balance, MIN_PROPOSAL_RAW)
  const canSubmit = !gated && !busy && Boolean(title.trim()) && sectionCheck.ok

  async function handleSubmit() {
    if (!address || !chain || !canSubmit) return
    setBusy(true)
    setError(null)
    try {
      const body_hash = await hashSections(sections)
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'propose',
        title,
        body_hash,
      })
      const created = await createProposal({ ...signed, title, body_sections: sections })
      navigate(ROUTES.proposal(created.id))
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Propose failed')
    } finally {
      setBusy(false)
    }
  }

  if (!address) {
    return (
      <div className="panel">
        <p>Connect a wallet to create a proposal.</p>
        <Link to={ROUTES.list}>Back to proposals</Link>
      </div>
    )
  }

  const banner = error || snapshot.error

  return (
    <div className="page-stack">
      <section className="panel">
        <h1>New proposal</h1>
        <p className="lede">
          Server is the source of truth for the {MIN_PROPOSAL_CL8Y} CL8Y gate and required sections. You
          sign a hash of the canonical section JSON; operator-voting checks that hash, then stores
          ammonia-sanitized HTML per section. {label} and the other chain are never summed. Never enter a
          seed phrase.
        </p>
        {registered && snapshot.balance != null && (
          <p data-testid="propose-balance">
            Balance {formatCl8y(snapshot.balance)} CL8Y {gated ? `(need ${MIN_PROPOSAL_CL8Y})` : ''}
          </p>
        )}
        {snapshot.status === 'unregistered' && (
          <p className="lede" data-testid="propose-register-hint">
            Register to snapshot this address’s CL8Y before proposing. The list page does not show a live
            chain balance.
          </p>
        )}
        {snapshot.status === 'pending' && (
          <p className="lede" data-testid="propose-pending">
            Registration snapshot pending. Propose stays disabled until the ledger row exists.
          </p>
        )}
        <label className="field">
          <span>Title</span>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            maxLength={200}
            data-testid="proposal-title"
          />
        </label>
        <ProposalSectionFields
          values={plain}
          errors={sectionCheck.errors}
          disabled={busy}
          onChange={(key, value) => setPlain((prev) => ({ ...prev, [key]: value }))}
        />
        <ProposalSectionsPreview sections={sections} />
        {banner && (
          <div className="alert-error" role="alert">
            {banner}
          </div>
        )}
        <button
          type="button"
          className="btn-primary"
          data-testid="submit-proposal"
          disabled={!canSubmit}
          onClick={() => void handleSubmit()}
        >
          {busy ? 'Signing…' : 'Create proposal'}
        </button>
      </section>
    </div>
  )
}
