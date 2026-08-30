import { useMemo, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { useVotingSnapshot } from '@/hooks/useVotingSnapshot'
import { createProposal } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import { sha256Hex } from '@/utils/sanitizeProposalHtml'
import { canPropose } from '@/utils/votingPayload'
import { MIN_PROPOSAL_CL8Y, MIN_PROPOSAL_RAW } from '@/utils/constants'
import { formatCl8y } from '@/utils/format'
import { ROUTES } from '@/routes'
import {
  CANONICAL_SECTION_KEYS,
  canonicalizeProposalSections,
  emptySections,
  MIN_SECTION_VISIBLE,
  SECTION_HELP,
  SECTION_LABELS,
  sanitizeSections,
  sectionsAreValid,
  textToSectionHtml,
  visibleLen,
  type ProposalSections,
  type SectionKey,
} from '@/utils/proposalSections'

function sectionsFromText(raw: Record<SectionKey, string>): ProposalSections {
  const out = emptySections()
  for (const key of CANONICAL_SECTION_KEYS) {
    out[key] = textToSectionHtml(raw[key] ?? '')
  }
  return sanitizeSections(out)
}

export default function VoteNewPage() {
  const { address, chain, label } = useConnectedIdentity()
  const snapshot = useVotingSnapshot(address, chain)
  const navigate = useNavigate()
  const [title, setTitle] = useState('')
  const [fields, setFields] = useState<Record<SectionKey, string>>(emptySections())
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const registered = snapshot.status === 'registered'
  const gated = !registered || snapshot.balance == null || !canPropose(snapshot.balance, MIN_PROPOSAL_RAW)
  const sections = useMemo(() => sectionsFromText(fields), [fields])
  const templateOk = sectionsAreValid(sections)

  async function handleSubmit() {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      const body_hash = await sha256Hex(canonicalizeProposalSections(sections))
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'draft',
        title,
        body_hash,
      })
      const created = await createProposal({ ...signed, title, sections })
      navigate(ROUTES.proposal(created.id))
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Draft failed')
    } finally {
      setBusy(false)
    }
  }

  if (!address) {
    return (
      <div className="panel">
        <p>Connect a wallet to start a draft.</p>
        <Link to={ROUTES.list}>Back to proposals</Link>
      </div>
    )
  }

  const banner = error || snapshot.error

  return (
    <div className="page-stack">
      <section className="panel">
        <h1>New draft</h1>
        <p className="lede">
          Starts as a <strong>draft</strong> — not a votable poll. Snapshot freeze happens when a committee wallet
          opens voting. Server enforces the {MIN_PROPOSAL_CL8Y} CL8Y gate on this address only. {label} and the
          other chain are never summed. Required sections need at least {MIN_SECTION_VISIBLE} visible characters.
        </p>
        {registered && snapshot.balance != null && (
          <p data-testid="propose-balance">
            Balance {formatCl8y(snapshot.balance)} CL8Y {gated ? `(need ${MIN_PROPOSAL_CL8Y})` : ''}
          </p>
        )}
        {snapshot.status === 'unregistered' && (
          <p className="lede" data-testid="propose-register-hint">
            Register to snapshot this address’s CL8Y before starting a draft. The list page does not show a live
            chain balance.
          </p>
        )}
        {snapshot.status === 'pending' && (
          <p className="lede" data-testid="propose-pending">
            Registration snapshot pending. Draft stays disabled until the ledger row exists.
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
        {CANONICAL_SECTION_KEYS.map((key) => (
          <label className="field" key={key}>
            <span>
              {SECTION_LABELS[key]}
              {key === 'context' ? ' (optional)' : ''}
            </span>
            <p className="lede">{SECTION_HELP[key]}</p>
            <textarea
              value={fields[key]}
              onChange={(e) => setFields((prev) => ({ ...prev, [key]: e.target.value }))}
              rows={key === 'summary' ? 3 : 5}
              data-testid={`section-${key}`}
            />
            <span className="lede">
              {visibleLen(textToSectionHtml(fields[key]))} visible
              {key !== 'context' ? ` / ${MIN_SECTION_VISIBLE} min` : ''}
            </span>
          </label>
        ))}
        {banner && (
          <div className="alert-error" role="alert">
            {banner}
          </div>
        )}
        <button
          type="button"
          className="btn-primary"
          data-testid="submit-proposal"
          disabled={gated || busy || !title.trim() || !templateOk}
          onClick={() => void handleSubmit()}
        >
          {busy ? 'Signing…' : 'Create draft'}
        </button>
      </section>
    </div>
  )
}
