interface WalletOptionIconProps {
  src: string
  /** Decorative when the row already exposes an accessible name. */
  decorative?: boolean
  testId?: string
}

/**
 * Circular brand mark for a Connect Wallet row (#490).
 * Callers keep the wallet name as visible text + `aria-label` on the button.
 */
export function WalletOptionIcon({ src, decorative = true, testId }: WalletOptionIconProps) {
  return (
    <span className="wallet-option-icon" data-testid={testId} aria-hidden={decorative || undefined}>
      <img
        className="wallet-option-icon-img"
        src={src}
        alt=""
        width={32}
        height={32}
        decoding="async"
        draggable={false}
      />
    </span>
  )
}
