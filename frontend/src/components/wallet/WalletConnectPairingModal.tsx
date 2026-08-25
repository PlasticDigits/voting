import { CopyButton } from '@/components/ui/CopyButton'
import { Modal } from '@/components/ui'
import { useWalletStore } from '@/hooks/useWallet'
import { useWalletConnectPairingStore } from '@/hooks/useWalletConnectPairingStore'
import { buildWalletConnectDeepLinks, isAllowedWalletConnectDeepLink } from '@/utils/walletConnectPairing'

export default function WalletConnectPairingModal() {
  const { isOpen, payload, close } = useWalletConnectPairingStore()
  const cancelConnection = useWalletStore((s) => s.cancelConnection)

  if (!isOpen || !payload) return null

  const links = buildWalletConnectDeepLinks(payload, payload.uri)

  function handleUserDismiss() {
    close()
    cancelConnection()
  }

  return (
    <Modal
      isOpen={true}
      onClose={handleUserDismiss}
      title={`Connect ${payload.name}`}
      zIndexClassName="z-[10001]"
      rootTestId="walletconnect-pairing-portal"
    >
      <div className="walletconnect-pairing px-6 py-4" data-testid="walletconnect-pairing-modal">
        <p className="mb-4 text-sm" style={{ color: 'var(--ink-subtle)' }}>
          Open your wallet, then return here.
        </p>
        <div className="flex flex-col gap-2">
          {links.map((link) => (
            <a
              key={link.id}
              className={
                link.id === 'wallet' ? 'btn-primary walletconnect-pairing-link' : 'btn-muted walletconnect-pairing-link'
              }
              href={isAllowedWalletConnectDeepLink(link.href) ? link.href : undefined}
              data-testid={`walletconnect-pairing-${link.id}`}
              onClick={(event) => {
                if (!isAllowedWalletConnectDeepLink(link.href)) {
                  event.preventDefault()
                }
              }}
            >
              {link.label}
            </a>
          ))}
          <CopyButton
            text={payload.uri}
            ariaLabel="Copy pairing link"
            buttonLabel="Copy pairing link"
            data-testid="walletconnect-pairing-copy"
          />
          <button type="button" className="btn-muted" data-testid="walletconnect-pairing-cancel" onClick={handleUserDismiss}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
  )
}
