import { useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { EditorContent, useEditor } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { useVotingSnapshot } from '@/hooks/useVotingSnapshot'
import { createProposal } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import { sanitizeProposalHtml, sha256Hex } from '@/utils/sanitizeProposalHtml'
import { canPropose } from '@/utils/votingPayload'
import { MIN_PROPOSAL_CL8Y, MIN_PROPOSAL_RAW } from '@/utils/constants'
import { formatCl8y } from '@/utils/format'

export default function VoteNewPage() {
  const { address, chain, label } = useConnectedIdentity()
  const snapshot = useVotingSnapshot(address, chain)
  const navigate = useNavigate()
  const [title, setTitle] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const editor = useEditor({
    extensions: [StarterKit],
    content: '<p></p>',
  })

  const registered = snapshot.status === 'registered'
  const gated = !registered || snapshot.balance == null || !canPropose(snapshot.balance, MIN_PROPOSAL_RAW)

  async function handleSubmit() {
    if (!address || !chain || !editor) return
    setBusy(true)
    setError(null)
    try {
      const rawHtml = editor.getHTML()
      const body_html = sanitizeProposalHtml(rawHtml)
      const body_hash = await sha256Hex(body_html)
      const signed = await signVotingRequest({
        chain,
        address,
        purpose: 'propose',
        title,
        body_hash,
      })
      const created = await createProposal({ ...signed, title, body_html })
      navigate(`/vote/${created.id}`)
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
        <Link to="/vote">Back to proposals</Link>
      </div>
    )
  }

  const banner = error || snapshot.error

  return (
    <div className="page-stack">
      <section className="panel">
        <h1>New proposal</h1>
        <p className="lede">
          Server is the source of truth for the {MIN_PROPOSAL_CL8Y} CL8Y gate. You sign a hash of the HTML you submit;
          operator-voting checks that hash, then stores ammonia-sanitized HTML. {label} and the other chain are never
          summed.
        </p>
        {registered && snapshot.balance != null && (
          <p data-testid="propose-balance">
            Balance {formatCl8y(snapshot.balance)} CL8Y {gated ? `(need ${MIN_PROPOSAL_CL8Y})` : ''}
          </p>
        )}
        {snapshot.status === 'unregistered' && (
          <p className="lede" data-testid="propose-register-hint">
            Register to snapshot this address’s CL8Y before proposing. The list page does not show a live chain
            balance.
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
        <div className="field">
          <span>Body</span>
          <div className="editor" data-testid="proposal-body">
            <EditorContent editor={editor} />
          </div>
        </div>
        {banner && (
          <div className="alert-error" role="alert">
            {banner}
          </div>
        )}
        <button
          type="button"
          className="btn-primary"
          data-testid="submit-proposal"
          disabled={gated || busy || !title.trim()}
          onClick={() => void handleSubmit()}
        >
          {busy ? 'Signing…' : 'Create proposal'}
        </button>
      </section>
    </div>
  )
}
