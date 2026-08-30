import { WalletName, WalletType } from '@goblinhunt/cosmes/wallet'
import { useWalletStore } from '@/hooks/useWallet'
import { useWalletConnectPairingStore } from '@/hooks/useWalletConnectPairingStore'
import { useWalletExtensionInstallSnapshot } from '@/hooks/useWalletExtensionInstallSnapshot'
import { useEvmWalletDiscovery } from '@/hooks/useEvmWalletDiscovery'
import { useEvmWalletStore } from '@/stores/evmWallet'
import { DEV_MODE, WC_PROJECT_ID } from '@/utils/constants'
import { Modal } from '@/components/ui'
import { WALLET_EXTENSION_INSTALL_URL } from '@/services/terraclassic/walletExtensionInstall'
import { detectWalletInAppBrowser } from '@/utils/detectWalletInAppBrowser'
import { isWalletConnectMobileClient } from '@/utils/walletConnectPairing'
import { resolveConnectWalletOptions, type ConnectWalletOption } from './connectWalletOptions'
import { WalletOptionIcon } from './WalletOptionIcon'
import { SIMULATED_WALLET_ICON_SRC, walletIconSrc } from './walletIconSrc'

interface WalletModalProps {
  onClose: () => void
}

export default function WalletModal({ onClose }: WalletModalProps) {
  const { connect, connectDev, isConnecting, error, cancelConnection } = useWalletStore()
  const pairingOpen = useWalletConnectPairingStore((s) => s.isOpen)
  const extensionInstall = useWalletExtensionInstallSnapshot()
  const isMobileClient = isWalletConnectMobileClient()
  const options = resolveConnectWalletOptions({
    isMobileClient,
    keplrInjected: extensionInstall.get(WalletName.KEPLR) ?? false,
    stationInjected: extensionInstall.get(WalletName.STATION) ?? false,
    cosmostationInjected: extensionInstall.get(WalletName.COSMOSTATION) ?? false,
    walletConnectConfigured: Boolean(WC_PROJECT_ID),
  })
  const showMobileHint = isMobileClient && !detectWalletInAppBrowser().isInAppBrowser
  const { connectors } = useEvmWalletDiscovery()
  const evmConnect = useEvmWalletStore((s) => s.connect)
  const evmConnecting = useEvmWalletStore((s) => s.connecting)
  const evmError = useEvmWalletStore((s) => s.error)
  const disconnectTerra = useWalletStore((s) => s.disconnect)

  if (pairingOpen) return null

  async function handleConnect(option: ConnectWalletOption) {
    await connect(option.walletName, option.walletType)
    const state = useWalletStore.getState()
    if (state.address && !state.error) onClose()
  }

  function handleDevConnect() {
    connectDev()
    onClose()
  }

  async function handleEvmConnect(connector: { id: string; type: string }) {
    await disconnectTerra()
    // Bridge EvmWalletModal: close our overlay so the WalletConnect QR iframe
    // is not trapped under Connect (z-[9999] vs Reown modal).
    if (connector.type === 'walletConnect') {
      useWalletStore.getState().setWalletModalOpen(false)
    }
    await evmConnect(connector.id)
    if (useEvmWalletStore.getState().address) onClose()
  }

  function handleClose() {
    if (isConnecting || evmConnecting) {
      cancelConnection()
      return
    }
    onClose()
  }

  return (
    <Modal isOpen={true} onClose={handleClose} title="Connect Wallet" rootTestId="wallet-connect-modal-portal">
      <div className="px-6 py-4">
        {(error || evmError) && <div className="alert-error mb-4">{error || evmError}</div>}

        {showMobileHint ? (
          <p className="mb-3 text-sm" style={{ color: 'var(--ink-subtle)' }} data-testid="wallet-modal-mobile-hint">
            Use Open or Copy next. Wallet browser also works.
          </p>
        ) : null}

        <p className="mb-2 text-xs uppercase tracking-wide" style={{ color: 'var(--gold)' }}>
          Terra Classic
        </p>
        <div className="space-y-2">
          {DEV_MODE && (
            <button
              onClick={handleDevConnect}
              className="wallet-option-card wallet-option-card-dev"
              aria-label="Simulated Wallet"
              data-testid="connect-simulated-terra"
            >
              <span className="wallet-option-main">
                <WalletOptionIcon src={SIMULATED_WALLET_ICON_SRC} testId="wallet-option-icon-simulated" />
                <span className="font-medium uppercase tracking-wide text-sm" style={{ color: '#ffd28d' }}>
                  Simulated Wallet
                </span>
              </span>
              <span className="wallet-option-badge wallet-option-badge-dev">DEV</span>
            </button>
          )}

          {options.map((option) => {
            const isExtension = option.walletType === WalletType.EXTENSION
            const installed = !isExtension || (extensionInstall.get(option.walletName) ?? false)
            const installUrl = isExtension ? WALLET_EXTENSION_INSTALL_URL[option.walletName] : undefined
            const iconSrc = walletIconSrc(option.walletName)

            return (
              <div key={option.name} className="wallet-option-row">
                <button
                  type="button"
                  onClick={() => void handleConnect(option)}
                  disabled={isConnecting}
                  className={
                    isExtension && !installed
                      ? 'wallet-option-card wallet-option-card-unavailable disabled:opacity-50'
                      : 'wallet-option-card disabled:opacity-50'
                  }
                  aria-label={
                    isExtension
                      ? `${option.name}, ${installed ? 'extension detected' : 'extension not detected'}`
                      : option.name
                  }
                >
                  <span className="wallet-option-main">
                    {iconSrc ? <WalletOptionIcon src={iconSrc} testId={`wallet-option-icon-${option.walletName}`} /> : null}
                    <span className="flex min-w-0 flex-1 flex-col items-start gap-1 text-left">
                      <span
                        className="max-w-full truncate font-medium uppercase tracking-wide text-sm"
                        style={{ color: 'var(--ink)' }}
                        title={option.name}
                      >
                        {option.name}
                      </span>
                    </span>
                  </span>
                  <span className="flex shrink-0 flex-wrap items-center justify-end gap-1.5 sm:gap-2">
                    <span className="wallet-option-badge">{option.connectionLabel}</span>
                    {isExtension && installed ? (
                      <span className="wallet-option-badge wallet-option-badge-ready">Ready</span>
                    ) : null}
                  </span>
                </button>
                {isExtension && !installed && installUrl ? (
                  <a
                    className="wallet-option-install-cta"
                    href={installUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={(e) => e.stopPropagation()}
                  >
                    Install
                  </a>
                ) : null}
              </div>
            )
          })}
        </div>

        <p className="mb-2 mt-5 text-xs uppercase tracking-wide" style={{ color: 'var(--gold)' }}>
          BSC (EVM)
        </p>
        <div className="space-y-2">
          {connectors.map((connector) => (
            <button
              key={connector.uid}
              type="button"
              className="wallet-option-card disabled:opacity-50"
              disabled={evmConnecting}
              data-testid={`connect-evm-${connector.type}`}
              onClick={() => void handleEvmConnect(connector)}
            >
              <span className="wallet-option-main">
                <span className="font-medium uppercase tracking-wide text-sm" style={{ color: 'var(--ink)' }}>
                  {connector.name}
                </span>
              </span>
              <span className="wallet-option-badge">{connector.type === 'mock' ? 'DEV' : 'EVM'}</span>
            </button>
          ))}
        </div>

        {(isConnecting || evmConnecting) && (
          <div className="mt-4 flex flex-col items-center gap-2">
            <div className="text-center text-sm uppercase tracking-wide" style={{ color: 'var(--ink-subtle)' }}>
              Connecting...
            </div>
            <button type="button" className="btn-muted" data-testid="wallet-connect-cancel" onClick={cancelConnection}>
              Cancel
            </button>
          </div>
        )}
        <p className="mt-4 text-xs" style={{ color: 'var(--ink-subtle)' }}>
          One address at a time. Connecting a wallet disconnects the other chain. Never enter a seed phrase here.
        </p>
      </div>
    </Modal>
  )
}
