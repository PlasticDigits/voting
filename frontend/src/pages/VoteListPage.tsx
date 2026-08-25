import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { getBalance, getRegistration, listProposals, registerWallet, type ProposalListItem } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import { formatCl8y } from '@/utils/format'

export default function VoteListPage() {
  const { address, chain, label } = useConnectedIdentity()
  const [proposals, setProposals] = useState<ProposalListItem[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [registered, setRegistered] = useState<boolean | null>(null)
  const [balance, setBalance] = useState<string | null>(null)

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

  useEffect(() => {
    if (!address || !chain) {
      setRegistered(null)
      setBalance(null)
      return
    }
    let cancelled = false
    void Promise.all([getRegistration(address), getBalance(address, chain)])
      .then(([reg, bal]) => {
        if (cancelled) return
        setRegistered(Boolean(reg && (reg[chain] || Object.keys(reg).length > 0)))
        setBalance(bal.balance)
      })
      .catch(() => {
        if (!cancelled) {
          setRegistered(false)
          setBalance(null)
        }
      })
    return () => {
      cancelled = true
    }
  }, [address, chain])

  async function handleRegister() {
    if (!address || !chain) return
    setBusy(true)
    setError(null)
    try {
      const signed = await signVotingRequest({ chain, address, purpose: 'register' })
      await registerWallet(signed)
      setRegistered(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Registration failed')
    } finally {
      setBusy(false)
    }
  }

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
          <div className="cta-row">
            <p data-testid="connected-chain">
              Connected on <strong>{label}</strong>
              {balance != null ? ` · ${formatCl8y(balance)} CL8Y` : ''}
            </p>
            {registered ? (
              <span className="pill" data-testid="registration-status">
                Registered
              </span>
            ) : (
              <button
                type="button"
                className="btn-primary"
                data-testid="register-cta"
                disabled={busy}
                onClick={() => void handleRegister()}
              >
                {busy ? 'Signing…' : `Register on ${label}`}
              </button>
            )}
            <Link className="btn-primary" to="/vote/new" data-testid="propose-cta">
              New proposal
            </Link>
          </div>
        ) : (
          <p className="lede">Connect a Terra Classic or BSC wallet to register and vote.</p>
        )}
        {error && (
          <div className="alert-error" role="alert">
            {error}
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
