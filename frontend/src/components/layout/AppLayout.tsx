import { Outlet } from 'react-router-dom'
import WalletButton from '@/components/wallet/WalletButton'
import WalletConnectPairingModal from '@/components/wallet/WalletConnectPairingModal'
import ConnectedTermsGate from '@/components/legal/ConnectedTermsGate'

export default function AppLayout() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="app-header-inner">
          <div>
            <p className="app-brand">CL8Y Voting</p>
            <p className="app-brand-sub">Offchain · advisory snapshots</p>
          </div>
          <WalletButton />
        </div>
      </header>
      <main className="app-main">
        <ConnectedTermsGate>
          <Outlet />
        </ConnectedTermsGate>
      </main>
      <WalletConnectPairingModal />
    </div>
  )
}
