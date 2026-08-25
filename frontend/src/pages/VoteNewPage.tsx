import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { EditorContent, useEditor } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { createProposal, getBalance } from '@/services/operatorVoting'
import { signVotingRequest } from '@/services/votingSign'
import { sanitizeProposalHtml, sha256Hex } from '@/utils/sanitizeProposalHtml'
import { canPropose } from '@/utils/votingPayload'
import { MIN_PROPOSAL_CL8Y, MIN_PROPOSAL_RAW } from '@/utils/constants'
import { formatCl8y } from '@/utils/format'

export default function VoteNewPage() {
  const { address, chain } = useConnectedIdentity()
  const navigate = useNavigate()
  const [title, setTitle] = useState('')
  const [balance, setBalance] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const editor = useEditor({
    extensions: [StarterKit],
    content: '<p></p>',
  })

  useEffect(() => {
    if (!address || !chain) {
      setBalance(null)
      return
    }
    let cancelled = false
    void getBalance(address, chain)
      .then((bal) => {
        if (!cancelled) setBalance(bal.balance)
      })
      .catch(() => {
        if (!cancelled) setBalance(null)
      })
    return () => {
      cancelled = true
    }
  }, [address, chain])

  const gated = balance == null || !canPropose(balance, MIN_PROPOSAL_RAW)

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

  return (
    <div className="page-stack">
      <section className="panel">
        <h1>New proposal</h1>
        <p className="lede">
          Server is the source of truth for the {MIN_PROPOSAL_CL8Y} CL8Y gate. You sign a hash of the HTML you submit;
          operator-voting sanitizes then compares.
        </p>
        {balance != null && (
          <p data-testid="propose-balance">
            Balance {formatCl8y(balance)} CL8Y {gated ? `(need ${MIN_PROPOSAL_CL8Y})` : ''}
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
        {error && (
          <div className="alert-error" role="alert">
            {error}
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
