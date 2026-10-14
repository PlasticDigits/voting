import SafeHtml from '@/components/proposal/SafeHtml'
import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import {
  amendProposal,
  canonicalizeForSigning,
  castVote,
  getProposal,
  getFrozenWeight,
  getRecordedVote,
  type FrozenWeightResponse,
  openProposal,
  postAnalysis,
  postComment,
  type ProposalDetail,
} from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import type { VoteChoice } from '@/utils/votingPayload'
import { sanitizeProposalHtml } from '@/utils/sanitizeProposalHtml'
import { formatCl8y } from '@/utils/format'
import { ROUTES } from '@/routes'
import { ProposalSectionView } from '@/components/proposal/ProposalSectionFields'
import {
  ANALYSIS_LABELS,
  CANONICAL_ANALYSIS_KEYS,
  CANONICAL_SECTION_KEYS,
  SECTION_DISPLAY_ORDER,
  emptyAnalysis,
  emptySections,
  SECTION_LABELS,
  sanitizeAnalysis,
  sanitizeSections,
  sectionsAreValid,
  textToSectionHtml,
  type AnalysisKey,
  type AnalysisSections,
  type ProposalSections,
  type SectionKey,
} from '@/utils/proposalSections'

function sameAddress(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase()
}

function htmlFromFields(raw: Record<SectionKey, string>): ProposalSections {
  const out = emptySections()
  for (const key of CANONICAL_SECTION_KEYS) {
    out[key] = textToSectionHtml(raw[key] ?? '')
  }
  return sanitizeSections(out)
}

function analysisFromFields(raw: Record<AnalysisKey, string>): AnalysisSections {
  const out = emptyAnalysis()
  for (const key of CANONICAL_ANALYSIS_KEYS) {
    out[key] = textToSectionHtml(raw[key] ?? '')
  }
  return sanitizeAnalysis(out)
}

export default function VoteDetailPage() {
  const { id = '' } = useParams()
  const { address, chain } = useConnectedIdentity()
  const contextKey = `${id}:${chain ?? ''}:${address?.trim().toLowerCase() ?? ''}`
  const currentContext = useRef(contextKey)
  currentContext.current = contextKey
  const [proposal, setProposal] = useState<ProposalDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [frozenWeight, setFrozenWeight] = useState<FrozenWeightResponse | null>(null)
  const [weightError, setWeightError] = useState<string | null>(null)
  const [weightRetry, setWeightRetry] = useState(0)
  const [voted, setVoted] = useState<{
    choice: VoteChoice
    address: string
    chain: string
    proposalId: string
  } | null>(null)
  const [comment, setComment] = useState('')
  const [amendFields, setAmendFields] = useState<Record<SectionKey, string> | null>(null)
  const [analysisFields, setAnalysisFields] = useState<Record<AnalysisKey, string>>(emptyAnalysis())

  async function reload() {
    const p = await getProposal(id)
    if (currentContext.current === contextKey) setProposal(p)
    return p
  }

  useEffect(() => {
    let cancelled = false
    void getProposal(id)
      .then((p) => {
        if (!cancelled) setProposal(p)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Failed to load proposal')
      })
    return () => {
      cancelled = true
    }
  }, [id])

  const isProposer = Boolean(address && proposal && sameAddress(address, proposal.proposer))
  const isDraft = proposal?.status === 'draft'
  const isOpen = proposal?.status === 'open'
  const canVote = isOpen && proposal?.snapshot_validity === 'valid'

  const hasRecordedVote = Boolean(
    voted &&
      voted.proposalId === id &&
      address &&
      chain === voted.chain &&
      sameAddress(address, voted.address),
  )
  const hasFrozenWeight = Boolean(
    address && chain && frozenWeight && usableFrozenWeight(frozenWeight, id, address, chain),
  )
  useEffect(() => {
    setFrozenWeight(null)
    setWeightError(null)
    if (!isOpen || !address || !chain) return
    let cancelled = false
    void Promise.all([getFrozenWeight(id, address, chain), getRecordedVote(id, address, chain)])
      .then(([weight, recorded]) => {
        if (!cancelled) {
          setFrozenWeight(weight)
          setVoted(recorded ? { choice: recorded.choice, address, chain, proposalId: id } : null)
        }
      })
      .catch(() => {
        if (!cancelled)
          setWeightError('Unable to check frozen voting weight and prior vote. Refresh and retry.')
      })
    return () => {
      cancelled = true
    }
  }, [id, address, chain, isOpen, weightRetry])

  const amendSections = useMemo(
    () => (amendFields ? htmlFromFields(amendFields) : null),
    [amendFields],
  )

  async function handleVote(choice: VoteChoice) {
    if (!address || !chain || !canVote || !hasFrozenWeight || hasRecordedVote) return
    setBusy(true)
    setError(null)
    try {
      const [weight, recorded] = await Promise.all([
        getFrozenWeight(id, address, chain),
        getRecordedVote(id, address, chain),
      ])
      setFrozenWeight(weight)
      if (recorded) {
        setVoted({ choice: recorded.choice, address, chain, proposalId: id })
        await reload()
        return
      }
      if (!usableFrozenWeight(weight, id, address, chain)) {
        setError('This address has no eligible frozen voting weight in this poll.')
        await reload()
        return
      }
      if (currentContext.current !== contextKey) return
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'vote',
        proposal_id: id,
        choice,
      })
      await castVote(id, { ...signed, choice })
      setVoted({ choice, address, chain, proposalId: id })
      await reload().catch(() => setError('Vote recorded. Refresh to load the updated tally.'))
    } catch (err) {
      // A response can be lost after commit. Read immutable state before offering
      // another signature, and keep the recorded choice attached to its voter.
      const recorded = await getRecordedVote(id, address, chain).catch(() => null)
      if (recorded) {
        setVoted({ choice: recorded.choice, address, chain, proposalId: id })
        await reload().catch(() => setError('Vote recorded. Refresh to load the updated tally.'))
      } else {
        setFrozenWeight(null)
        setWeightError(
          'Vote submission could not be verified. Refresh voting weight and recorded state before signing again.',
        )
        setError(err instanceof Error ? err.message : 'Vote failed')
      }
    } finally {
      setBusy(false)
    }
  }

  async function handleComment() {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      const canonical = await canonicalizeForSigning({
        kind: 'comment',
        body_html: textToSectionHtml(comment),
      })
      const { body_html, body_hash } = canonical
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'comment',
        proposal_id: id,
        body_hash,
      })
      await postComment(id, { ...signed, body_html })
      setComment('')
      await reload()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Comment failed')
    } finally {
      setBusy(false)
    }
  }

  async function handleAmend() {
    if (!address || !chain || !proposal || !amendSections) return
    setBusy(true)
    setError(null)
    try {
      if (!proposal.body_hash)
        throw new Error('Proposal canonical content unavailable; reload before signing.')
      const prev_body_hash = proposal.body_hash
      const canonical = await canonicalizeForSigning({ kind: 'proposal', sections: amendSections })
      const { body_hash } = canonical
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'amend',
        revision: proposal.revision,
        title: proposal.title,
        proposal_id: id,
        body_hash,
        prev_body_hash,
      })
      await amendProposal(id, {
        ...signed,
        title: proposal.title,
        body_sections: canonical.sections as ProposalSections,
      })
      setAmendFields(null)
      await reload()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Amend failed')
    } finally {
      setBusy(false)
    }
  }

  async function handleOpen() {
    if (!address || !chain || !proposal?.body_sections) return
    setBusy(true)
    setError(null)
    try {
      if (!proposal.body_hash)
        throw new Error('Proposal canonical content unavailable; reload before signing.')
      const body_hash = proposal.body_hash
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'open_vote',
        title: proposal.title,
        revision: proposal.revision,
        proposal_id: id,
        body_hash,
      })
      await openProposal(id, signed)
      await reload()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Open failed')
    } finally {
      setBusy(false)
    }
  }

  async function handleAnalysis() {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      if (!proposal) return
      const canonical = await canonicalizeForSigning({
        kind: 'analysis',
        sections: analysisFromFields(analysisFields),
      })
      const sections = canonical.sections as AnalysisSections
      const { body_hash } = canonical
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'analyze',
        revision: proposal.revision,
        proposal_id: id,
        body_hash,
      })
      await postAnalysis(id, { ...signed, sections })
      setAnalysisFields(emptyAnalysis())
      await reload()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Analysis failed')
    } finally {
      setBusy(false)
    }
  }

  if ((!proposal || proposal.id !== id) && !error) {
    return <p className="panel">Loading…</p>
  }

  const freezeCopy =
    isOpen && proposal?.terra_height != null
      ? `Snapshot freeze Terra height ${proposal.terra_height}, BSC block ${proposal.bsc_block}. Registration must be certified at or before the finalized checkpoint chosen for this poll. Later registration eligibility does not change its frozen weight.`
      : 'This is a draft. Vote weight is not frozen yet — register before the committee opens the poll.'

  return (
    <div className="page-stack">
      <section className="panel">
        <Link to={ROUTES.list}>← All proposals</Link>
        {proposal && (
          <>
            <p className="pill" data-testid="proposal-status">
              {proposal.status}
            </p>
            <h1>{proposal.title}</h1>
            <p className="lede">
              Offchain / advisory. {freezeCopy} Identity v1 is one address, one voter.
            </p>
            {isOpen && !canVote && (
              <div className="alert-error" role="status" data-testid="snapshot-review-notice">
                <strong>
                  {proposal.snapshot_validity === 'invalid'
                    ? 'Snapshot invalidated'
                    : 'Snapshot unverified'}
                </strong>
                <p>
                  {proposal.snapshot_validity === 'legacy_unverified'
                    ? 'This poll opened before canonical snapshot evidence was recorded.'
                    : 'The snapshot needs review after a chain history or verification problem.'}{' '}
                  New votes are disabled. Existing votes and proposal history remain available for
                  review.
                </p>
                {proposal.snapshot_incident && (
                  <p data-testid="snapshot-incident">Incident: {proposal.snapshot_incident}</p>
                )}
              </div>
            )}
            <ProposalSectionView
              sections={proposal.body_sections}
              fallbackHtml={sanitizeProposalHtml(proposal.body_html)}
            />

            <h2>Independent analysis</h2>
            {(proposal.analysis ?? []).length === 0 ? (
              <p className="lede" data-testid="no-analysis">
                No independent analysis attached. Committee may open the poll anyway; this is not a
                proposer-authored “neutral review.”
              </p>
            ) : (
              (proposal.analysis ?? []).map((item) => (
                <article key={item.id} className="proposal-section" data-testid="analysis-item">
                  <p className="lede">
                    Signed by {item.wallet_address}
                    {item.machine_generated ? ' · machine-generated' : ''}
                  </p>
                  {CANONICAL_ANALYSIS_KEYS.map((key) => (
                    <div key={key}>
                      <h3>{ANALYSIS_LABELS[key]}</h3>
                      <SafeHtml className="proposal-body" html={item.sections?.[key] ?? ''} />
                    </div>
                  ))}
                </article>
              ))
            )}

            <h2>Comments</h2>
            <ul data-testid="comment-list">
              {(proposal.comments ?? []).map((c) => (
                <li key={c.id}>
                  <span className="lede">{c.wallet_address}</span>
                  <SafeHtml className="proposal-body" html={c.body_html} />
                </li>
              ))}
            </ul>
            {address && (
              <div className="field">
                <span>Add a review comment (registered wallets; not a vote)</span>
                <textarea
                  value={comment}
                  onChange={(e) => setComment(e.target.value)}
                  data-testid="comment-body"
                  rows={3}
                />
                <button
                  type="button"
                  className="btn-muted"
                  data-testid="submit-comment"
                  disabled={busy || !comment.trim()}
                  onClick={() => void handleComment()}
                >
                  Sign comment
                </button>
              </div>
            )}

            {isDraft && isProposer && (
              <div className="field">
                {amendFields ? (
                  <>
                    <h2>Amend draft sections</h2>
                    {SECTION_DISPLAY_ORDER.map((key) => (
                      <label className="field" key={key}>
                        <span>{SECTION_LABELS[key]}</span>
                        <textarea
                          value={amendFields[key]}
                          onChange={(e) =>
                            setAmendFields((prev) => ({
                              ...(prev ?? emptySections()),
                              [key]: e.target.value,
                            }))
                          }
                          data-testid={`amend-${key}`}
                          rows={4}
                        />
                      </label>
                    ))}
                    <button
                      type="button"
                      className="btn-primary"
                      data-testid="submit-amend"
                      disabled={busy || !amendSections || !sectionsAreValid(amendSections)}
                      onClick={() => void handleAmend()}
                    >
                      Sign amend
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="btn-muted"
                    data-testid="start-amend"
                    onClick={() => {
                      const next = emptySections()
                      for (const key of CANONICAL_SECTION_KEYS) {
                        next[key] = proposal.body_sections?.[key]
                          ? visibleTextFallback(proposal.body_sections[key])
                          : ''
                      }
                      setAmendFields(next)
                    }}
                  >
                    Amend sections
                  </button>
                )}
              </div>
            )}

            {isDraft && address && (
              <div className="field">
                <h2>Committee actions</h2>
                <p className="lede">
                  Only addresses in the server committee allowlist can attach independent analysis
                  or open the poll. The proposer cannot write the independent analysis record.
                  Opening does not require 1000 CL8Y. Non-allowlisted signatures return 403.
                </p>
                {CANONICAL_ANALYSIS_KEYS.map((key) => (
                  <label className="field" key={key}>
                    <span>{ANALYSIS_LABELS[key]}</span>
                    <textarea
                      value={analysisFields[key]}
                      onChange={(e) =>
                        setAnalysisFields((prev) => ({ ...prev, [key]: e.target.value }))
                      }
                      data-testid={`analysis-${key}`}
                      rows={3}
                    />
                  </label>
                ))}
                <button
                  type="button"
                  className="btn-muted"
                  data-testid="submit-analysis"
                  disabled={busy}
                  onClick={() => void handleAnalysis()}
                >
                  Sign independent analysis
                </button>
                <button
                  type="button"
                  className="btn-primary"
                  data-testid="open-poll"
                  disabled={busy}
                  onClick={() => void handleOpen()}
                >
                  Open poll (freeze snapshot)
                </button>
              </div>
            )}

            {isOpen && (
              <section data-testid="proposal-tally">
                <h2>Recorded votes</h2>
                {proposal.tally.length === 0 ? (
                  <p>No votes recorded.</p>
                ) : (
                  <table>
                    <thead>
                      <tr>
                        <th>Choice</th>
                        <th>Weight (CL8Y)</th>
                        <th>Wallets</th>
                      </tr>
                    </thead>
                    <tbody>
                      {proposal.tally.map((row) => (
                        <tr key={row.choice}>
                          <td>{row.choice}</td>
                          <td>{displayVoteWeight(row.weight)}</td>
                          <td>{row.voters}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </section>
            )}
            {canVote && address && (
              <div className="lede" data-testid="frozen-weight-status">
                {weightError ? (
                  <>
                    <p>{weightError}</p>
                    <button
                      type="button"
                      className="btn-muted"
                      onClick={() => setWeightRetry((at) => at + 1)}
                    >
                      Refresh voting weight
                    </button>
                  </>
                ) : frozenWeight == null ? (
                  <p>Checking frozen voting weight…</p>
                ) : !hasFrozenWeight ? (
                  <p>
                    This address has no eligible voting weight in this poll. Its registration may
                    have become eligible after the chosen finalized checkpoint. Register early for
                    future polls; existing frozen weights stay unchanged.
                  </p>
                ) : (
                  <p>Frozen voting weight: {displayVoteWeight(frozenWeight.weight)} CL8Y.</p>
                )}
              </div>
            )}
            {canVote && !hasRecordedVote && (
              <div className="cta-row" data-testid="vote-actions">
                {(['for', 'against', 'abstain'] as VoteChoice[]).map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    className="btn-primary"
                    data-testid={`vote-${choice}`}
                    disabled={!address || busy || !hasFrozenWeight}
                    onClick={() => void handleVote(choice)}
                  >
                    {choice}
                  </button>
                ))}
              </div>
            )}
            {voted &&
              voted.proposalId === id &&
              address &&
              chain === voted.chain &&
              sameAddress(address, voted.address) && (
                <p data-testid="vote-recorded">
                  Recorded: <strong>{voted.choice}</strong>
                </p>
              )}
          </>
        )}
        {error && (
          <div className="alert-error" role="alert">
            {error}
          </div>
        )}
      </section>
    </div>
  )
}

function visibleTextFallback(html: string): string {
  return html
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

function displayVoteWeight(raw: string): string {
  try {
    return formatCl8y(raw)
  } catch {
    return 'Unavailable'
  }
}

function usableFrozenWeight(
  weight: FrozenWeightResponse,
  proposalId: string,
  address: string,
  chain: string,
): boolean {
  return (
    weight.eligible &&
    weight.proposal_id === proposalId &&
    weight.chain === chain &&
    sameAddress(weight.address, address) &&
    typeof weight.weight === 'string' &&
    weight.weight.length <= 78 &&
    /^\d+$/.test(weight.weight) &&
    BigInt(weight.weight) > 0n
  )
}
