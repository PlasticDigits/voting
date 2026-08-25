import { useCallback, useState } from 'react'

export type CopyButtonProps = {
  text: string
  ariaLabel: string
  buttonLabel?: string
  'data-testid'?: string
}

export function CopyButton({
  text,
  ariaLabel,
  buttonLabel,
  'data-testid': testId = 'copy-button',
}: CopyButtonProps) {
  const [copied, setCopied] = useState(false)

  const handleClick = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      setCopied(false)
    }
  }, [text])

  return (
    <button
      type="button"
      className="btn-muted walletconnect-pairing-copy"
      aria-label={ariaLabel}
      data-testid={testId}
      onClick={() => void handleClick()}
    >
      {copied ? 'Copied' : (buttonLabel ?? 'Copy')}
    </button>
  )
}
