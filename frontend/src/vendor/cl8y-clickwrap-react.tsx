import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  createClient,
  NETWORK_API_VALUES,
  NETWORK_SIGN_URL_KEYS,
  type ClickwrapClient,
  type Network,
  type TermsLatest,
  buildSignUrl,
} from './cl8y-clickwrap'

export type TermsGateProps = {
  client?: ClickwrapClient
  property: string
  network: Network
  account: string | null | undefined
  redirectUri?: string
  appName?: string
  children: ReactNode
  fallback?: ReactNode
  unsigned?: ReactNode
  onError?: (error: Error) => void
}

export function TermsGate({
  client: clientProp,
  property,
  network,
  account,
  redirectUri,
  appName,
  children,
  fallback = <p>Checking terms acceptance…</p>,
  unsigned,
  onError,
}: TermsGateProps) {
  const client = useMemo(() => clientProp ?? createClient(), [clientProp])
  const [status, setStatus] = useState<{ signed_latest: boolean; signed_version: string | null } | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<Error | null>(null)
  const [terms, setTerms] = useState<TermsLatest | null>(null)
  const [termsError, setTermsError] = useState<Error | null>(null)

  useEffect(() => {
    if (!account?.trim()) {
      setLoading(false)
      return
    }
    let cancelled = false
    setLoading(true)
    setError(null)
    void client
      .getSignatureStatus(property, NETWORK_API_VALUES[network], account)
      .then((next) => {
        if (!cancelled) {
          setStatus(next)
          setLoading(false)
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          const wrapped = err instanceof Error ? err : new Error(String(err))
          setError(wrapped)
          setLoading(false)
          onError?.(wrapped)
        }
      })
    return () => {
      cancelled = true
    }
  }, [account, client, network, onError, property])

  const isSigned = Boolean(status?.signed_latest)

  useEffect(() => {
    if (loading || isSigned || !account?.trim()) return
    let cancelled = false
    void client
      .getTermsLatest(property)
      .then((latest) => {
        if (!cancelled) {
          setTerms(latest)
          setTermsError(null)
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          const wrapped = err instanceof Error ? err : new Error(String(err))
          setTermsError(wrapped)
          onError?.(wrapped)
        }
      })
    return () => {
      cancelled = true
    }
  }, [account, client, isSigned, loading, onError, property])

  const handleAccept = useCallback(() => {
    if (!terms) return
    const baseUrl = terms.sign_urls[NETWORK_SIGN_URL_KEYS[network]]
    window.location.href = buildSignUrl(baseUrl, { redirectUri, appName })
  }, [appName, network, redirectUri, terms])

  if (!account?.trim()) return <>{fallback}</>
  if (loading) return <>{fallback}</>
  if (error) {
    return <p role="alert">Unable to verify terms acceptance: {error.message}</p>
  }
  if (isSigned) return <>{children}</>
  if (unsigned) return <>{unsigned}</>

  const readTermsHref = `${client.apiBaseUrl}/api/v1/terms/latest/content?property=${encodeURIComponent(property)}`

  return (
    <div className="cl8y-clickwrap-gate">
      <h2>Accept Terms &amp; Conditions</h2>
      {terms ? (
        <p>
          You must accept CL8Y Terms &amp; Conditions <strong>{terms.version_label}</strong> (effective{' '}
          {terms.effective_date}) for <strong>{property}</strong>.
        </p>
      ) : termsError ? (
        <p role="alert">Unable to load terms: {termsError.message}</p>
      ) : (
        <p>Loading terms…</p>
      )}
      <p>
        <a href={readTermsHref} target="_blank" rel="noopener noreferrer">
          Read full terms
        </a>
      </p>
      <button type="button" onClick={handleAccept} disabled={!terms}>
        Accept Terms
      </button>
    </div>
  )
}
