import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { useVotingSnapshot } from '@/hooks/useVotingSnapshot'
import { listProposals, type ProposalListItem } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import { formatCl8y } from '@/utils/format'

export default function VoteListPage() {
  const { address, chain, label } = useConnectedIdentity()
  const snapshot = useVotingSnapshot(address, chain)
  const [proposals, setProposals] = useState<ProposalListItem[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let cancelled = false
    void listProposals()
      .then((rows) => {
        if (!cancelled) setProposals(rows)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Failed to load proposals')
      })
    return () => {
      cancelled = true
    }
  }, [])

  async function handleRegister() {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      const signed = await signVotingRequest({ chain, address, purpose: 'register' })
      await snapshot.completeRegister(signed)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Registration failed')
    } finally {
      setBusy(false)
    }
  }

  const banner = error || snapshot.error

  return (
    <div className="page-stack">
      <section className="panel">
        <h1>Proposals</h1>
        <p className="lede">
          Votes are offchain and advisory. Register the address that holds CL8Y on that chain{' '}
          <strong>before a proposal is created</strong>. Identity v1 is one address, one voter — Terra and BSC
          wallets are never merged. Never enter a seed phrase in this app.
        </p>
        {address && chain ? (
          <>
            <div className="cta-row">
              <p data-testid="connected-chain">
                Connected on <strong>{label}</strong>
                {snapshot.status === 'registered' && snapshot.balance != null
                  ? ` · ${formatCl8y(snapshot.balance)} CL8Y`
                  : ''}
              </p>
              {snapshot.status === 'registered' ? (
                <span className="pill" data-testid="registration-status">
                  Registered
                </span>
              ) : snapshot.status === 'pending' || busy ? (
                <span className="pill" data-testid="registration-pending">
                  Registering…
                </span>
              ) : (
                <button
                  type="button"
                  className="btn-primary"
                  data-testid="register-cta"
                  disabled={busy}
                  onClick={() => void handleRegister()}
                >
                  Register on {label}
                </button>
              )}
              <Link className="btn-primary" to="/vote/new" data-testid="propose-cta">
                New proposal
              </Link>
            </div>
            {snapshot.status === 'unregistered' && (
              <p className="lede" data-testid="register-hint">
                Register to snapshot this address’s CL8Y. The ledger is 0 until that live snapshot exists — this is
                not a wallet balance read.
              </p>
            )}
          </>
        ) : (
          <p className="lede">Connect a Terra Classic or BSC wallet to register and vote.</p>
        )}
        {banner && (
          <div className="alert-error" role="alert">
            {banner}
          </div>
        )}
      </section>
      <section className="panel">
        {proposals.length === 0 ? (
          <p data-testid="empty-proposals">No proposals yet.</p>
        ) : (
          <ul className="proposal-list" data-testid="proposal-list">
            {proposals.map((p) => (
              <li key={p.id}>
                <Link to={`/vote/${p.id}`}>
                  <strong>{p.title}</strong>
                  <span>
                    {p.chain} · {p.status}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}
