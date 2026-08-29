import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { castVote, getProposal, type ProposalDetail } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import type { VoteChoice } from '@/utils/votingPayload'
import { ROUTES } from '@/routes'

export default function VoteDetailPage() {
  const { id = '' } = useParams()
  const { address, chain } = useConnectedIdentity()
  const [proposal, setProposal] = useState<ProposalDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [voted, setVoted] = useState<VoteChoice | null>(null)

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

  if (!proposal && !error) {
    return <p className="panel">Loading…</p>
  }

  return (
    <div className="page-stack">
      <section className="panel">
        <Link to={ROUTES.list}>← All proposals</Link>
        {proposal && (
          <>
            <h1>{proposal.title}</h1>
            <p className="lede">
              Offchain / advisory. Snapshot freeze Terra height {proposal.terra_height}, BSC block {proposal.bsc_block}.
            </p>
            <article className="proposal-body" dangerouslySetInnerHTML={{ __html: proposal.body_html }} />
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
