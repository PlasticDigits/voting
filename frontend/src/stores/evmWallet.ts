import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { connect, disconnect, getAccount, getConnectors } from 'wagmi/actions'
import { config } from '@/lib/wagmi'

export type EvmWalletState = {
  connected: boolean
  connecting: boolean
  address: string | null
  connectorId: string | null
  chainId: number | null
  error: string | null
  connect: (connectorId: string) => Promise<void>
  disconnect: () => Promise<void>
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
        set({ connecting: true, error: null })
        try {
          const connector = getConnectors(config).find((c) => c.id === connectorId || c.uid === connectorId)
          if (!connector) {
            throw new Error('EVM connector not found')
          }
          const result = await connect(config, { connector })
          set({
            connected: true,
            connecting: false,
            address: result.accounts[0] ?? null,
            connectorId: connector.id,
            chainId: result.chainId,
            error: null,
          })
        } catch (error) {
          const message = error instanceof Error ? error.message : 'EVM connection failed'
          set({ connecting: false, error: message })
          throw error
        }
      },

      disconnect: async () => {
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
