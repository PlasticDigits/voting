import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useWalletStore } from '@/hooks/useWallet'
import { useEvmWalletStore } from '@/stores/evmWallet'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import { shortenAddress } from '@/utils/format'
import WalletModal from './WalletModal'

export default function WalletButton() {
  const { isConnecting, disconnect, walletModalOpen, setWalletModalOpen, closeWalletModal, cancelConnection } =
    useWalletStore()
  const evmDisconnect = useEvmWalletStore((s) => s.disconnect)
  const { address, label } = useConnectedIdentity()
  const [showDropdown, setShowDropdown] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)

  const handleDisconnect = async () => {
    await disconnect()
    await evmDisconnect()
    setShowDropdown(false)
  }

  useEffect(() => {
    if (!showDropdown) return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setShowDropdown(false)
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [showDropdown])

  if (address) {
    return (
      <>
        <div className="wallet-dropdown-wrap">
          <button
            ref={triggerRef}
            type="button"
            onClick={() => setShowDropdown(!showDropdown)}
            aria-haspopup="menu"
            aria-expanded={showDropdown}
            aria-label={`Connected wallet on ${label}`}
            className="wallet-trigger wallet-trigger-connected"
            data-testid="wallet-connected"
          >
            <div className="text-left min-w-0">
              <p className="text-[10px] uppercase tracking-[0.12em]" style={{ color: 'var(--gold)' }}>
                {label}
              </p>
              <p className="text-xs font-mono truncate" style={{ color: 'var(--ink)' }}>
                {shortenAddress(address)}
              </p>
            </div>
          </button>
          {showDropdown && (
            <>
              <button type="button" aria-label="Close wallet menu" className="app-menu-dismiss" onClick={() => setShowDropdown(false)} />
              <div className="wallet-menu">
                <div className="px-3 py-2 border-b border-white/10 space-y-1 min-w-0">
                  <p className="text-xs font-mono break-all" style={{ color: 'var(--ink-dim)' }}>
                    {address}
                  </p>
                </div>
                <button type="button" role="menuitem" className="wallet-menu-item" onClick={() => void handleDisconnect()}>
                  Disconnect
                </button>
              </div>
            </>
          )}
        </div>
        {walletModalOpen && createPortal(<WalletModal onClose={closeWalletModal} />, document.body)}
      </>
    )
  }

  return (
    <>
      <button
        type="button"
        onClick={() => {
          if (isConnecting) {
            cancelConnection()
            return
          }
          setWalletModalOpen(true)
        }}
        aria-label={isConnecting ? 'Cancel connecting' : 'Connect wallet'}
        className="btn-primary !px-3 !py-2"
        data-testid="wallet-connect"
      >
        {isConnecting ? 'Cancel' : 'Connect Wallet'}
      </button>
      {walletModalOpen && createPortal(<WalletModal onClose={closeWalletModal} />, document.body)}
    </>
  )
}
