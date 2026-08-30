import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { WagmiProvider } from 'wagmi'
import { Buffer } from 'buffer'
import App from './App'
import { config } from '@/lib/wagmi'
import { installWalletConnectPairingHook } from '@/services/terraclassic/walletConnectPairingHook'
import './index.css'

if (typeof window !== 'undefined') {
  window.Buffer = window.Buffer || Buffer
}

// WC-M6 / issue #12: register before createRoot so auto-reconnect and the first
// WalletConnect tap cannot race a useEffect. DEX main.tsx does the same.
installWalletConnectPairingHook()

const queryClient = new QueryClient()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <App />
        </BrowserRouter>
      </QueryClientProvider>
    </WagmiProvider>
  </StrictMode>
)
