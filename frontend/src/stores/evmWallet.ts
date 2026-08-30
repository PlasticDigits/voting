import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { connect, disconnect, getAccount, getConnectors } from 'wagmi/actions'
import { config } from '@/lib/wagmi'

/** Bumps on cancel so a late wagmi WalletConnect session cannot attach (WC-M9 / #12). */
let evmConnectAttemptId = 0

function abortInFlightEvmWalletConnect(): void {
  try {
    const account = getAccount(config)
    if (account.connector) {
      void disconnect(config, { connector: account.connector }).catch(() => undefined)
      return
    }
    for (const connector of getConnectors(config)) {
      if (connector.type === 'walletConnect') {
        void connector.disconnect?.().catch(() => undefined)
      }
    }
  } catch {
    /* non-fatal */
  }
}

export type EvmWalletState = {
  connected: boolean
  connecting: boolean
  address: string | null
  connectorId: string | null
  chainId: number | null
  error: string | null
  connect: (connectorId: string) => Promise<void>
  disconnect: () => Promise<void>
  /** Clears `connecting` and aborts in-flight WalletConnect (issue #12). */
  cancelConnection: () => void
  hydrateFromWagmi: () => void
  clearError: () => void
}

export const useEvmWalletStore = create<EvmWalletState>()(
  persist(
    (set) => ({
      connected: false,
      connecting: false,
      address: null,
      connectorId: null,
      chainId: null,
      error: null,

      hydrateFromWagmi: () => {
        const account = getAccount(config)
        if (account.address) {
          set({
            connected: true,
            address: account.address,
            connectorId: account.connector?.id ?? null,
            chainId: account.chainId ?? null,
          })
        }
      },

      connect: async (connectorId: string) => {
        const attempt = ++evmConnectAttemptId
        set({ connecting: true, error: null })
        try {
          const connector = getConnectors(config).find((c) => c.id === connectorId || c.uid === connectorId)
          if (!connector) {
            throw new Error('EVM connector not found')
          }
          const result = await connect(config, { connector })
          if (attempt !== evmConnectAttemptId) {
            try {
              await disconnect(config, { connector })
            } catch {
              /* late session after cancel must not stick */
            }
            return
          }
          set({
            connected: true,
            connecting: false,
            address: result.accounts[0] ?? null,
            connectorId: connector.id,
            chainId: result.chainId,
            error: null,
          })
        } catch (error) {
          if (attempt !== evmConnectAttemptId) {
            return
          }
          const message = error instanceof Error ? error.message : 'EVM connection failed'
          set({ connecting: false, error: message })
          throw error
        }
      },

      cancelConnection: () => {
        evmConnectAttemptId += 1
        abortInFlightEvmWalletConnect()
        set({ connecting: false, error: null })
      },

      disconnect: async () => {
        evmConnectAttemptId += 1
        try {
          const account = getAccount(config)
          if (account.connector) {
            await disconnect(config, { connector: account.connector })
          }
        } catch {
          /* non-fatal */
        }
        set({
          connected: false,
          connecting: false,
          address: null,
          connectorId: null,
          chainId: null,
        })
      },

      clearError: () => set({ error: null }),
    }),
    {
      name: 'cl8y-voting-evm-wallet',
      partialize: (state) => ({
        address: state.address,
        connectorId: state.connectorId,
      }),
    }
  )
)
