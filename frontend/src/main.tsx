import { StrictMode, useEffect } from 'react'
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

const queryClient = new QueryClient()

function Boot() {
  useEffect(() => installWalletConnectPairingHook(), [])
  return <App />
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Boot />
        </BrowserRouter>
      </QueryClientProvider>
    </WagmiProvider>
  </StrictMode>
)
