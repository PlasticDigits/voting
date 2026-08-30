import { useEffect, useMemo, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import {
  amendProposal,
  castVote,
  getProposal,
  openProposal,
  postAnalysis,
  postComment,
  type ProposalDetail,
} from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import type { VoteChoice } from '@/utils/votingPayload'
import { sanitizeProposalHtml, sha256Hex } from '@/utils/sanitizeProposalHtml'
import { ROUTES } from '@/routes'
import { ProposalSectionView } from '@/components/proposal/ProposalSectionFields'
import {
  ANALYSIS_LABELS,
  CANONICAL_ANALYSIS_KEYS,
  CANONICAL_SECTION_KEYS,
  SECTION_DISPLAY_ORDER,
  canonicalizeAnalysisSections,
  canonicalizeProposalSections,
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
  const [proposal, setProposal] = useState<ProposalDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [voted, setVoted] = useState<VoteChoice | null>(null)
  const [comment, setComment] = useState('')
  const [amendFields, setAmendFields] = useState<Record<SectionKey, string> | null>(null)
  const [analysisFields, setAnalysisFields] = useState<Record<AnalysisKey, string>>(emptyAnalysis())

  async function reload() {
    const p = await getProposal(id)
    setProposal(p)
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

  const amendSections = useMemo(
    () => (amendFields ? htmlFromFields(amendFields) : null),
    [amendFields]
  )

  async function handleVote(choice: VoteChoice) {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'vote',
        proposal_id: id,
        choice,
      })
      await castVote(id, { ...signed, choice })
      setVoted(choice)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Vote failed')
    } finally {
      setBusy(false)
    }
  }

  async function handleComment() {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      const body_html = textToSectionHtml(comment)
      const body_hash = await sha256Hex(body_html)
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
      const prev = proposal.body_sections
        ? canonicalizeProposalSections(sanitizeSections(proposal.body_sections))
        : ''
      const prev_body_hash = await sha256Hex(prev)
      const body_hash = await sha256Hex(canonicalizeProposalSections(amendSections))
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'amend',
        title: proposal.title,
        proposal_id: id,
        body_hash,
        prev_body_hash,
      })
      await amendProposal(id, { ...signed, title: proposal.title, body_sections: amendSections })
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
      const body_hash = await sha256Hex(
        canonicalizeProposalSections(sanitizeSections(proposal.body_sections))
      )
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'open_vote',
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
      const sections = analysisFromFields(analysisFields)
      const body_hash = await sha256Hex(canonicalizeAnalysisSections(sections))
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'analyze',
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

  if (!proposal && !error) {
    return <p className="panel">Loading…</p>
  }

  const freezeCopy =
    isOpen && proposal?.terra_height != null
      ? `Snapshot freeze Terra height ${proposal.terra_height}, BSC block ${proposal.bsc_block}. Register before this poll opened to be in the electorate.`
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
                      <div
                        className="proposal-body"
                        dangerouslySetInnerHTML={{ __html: item.sections?.[key] ?? '' }}
                      />
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
                  <div className="proposal-body" dangerouslySetInnerHTML={{ __html: c.body_html }} />
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
                          onChange={(e) => setAmendFields((prev) => ({ ...(prev ?? emptySections()), [key]: e.target.value }))}
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
                  Only addresses in the server committee allowlist can attach independent analysis or open
                  the poll. The proposer cannot write the independent analysis record. Opening does not
                  require 1000 CL8Y. Non-allowlisted signatures return 403.
                </p>
                {CANONICAL_ANALYSIS_KEYS.map((key) => (
                  <label className="field" key={key}>
                    <span>{ANALYSIS_LABELS[key]}</span>
                    <textarea
                      value={analysisFields[key]}
                      onChange={(e) => setAnalysisFields((prev) => ({ ...prev, [key]: e.target.value }))}
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
              <div className="cta-row" data-testid="vote-actions">
                {(['for', 'against', 'abstain'] as VoteChoice[]).map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    className="btn-primary"
                    data-testid={`vote-${choice}`}
                    disabled={!address || busy}
                    onClick={() => void handleVote(choice)}
                  >
                    {choice}
                  </button>
                ))}
              </div>
            )}
            {voted && (
              <p data-testid="vote-recorded">
                Recorded: <strong>{voted}</strong>
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
  return html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim()
}
