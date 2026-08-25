import { create } from 'zustand'
import type { WalletConnectPairingHookPayload } from '@/utils/walletConnectPairing'

type WalletConnectPairingState = {
  isOpen: boolean
  payload: WalletConnectPairingHookPayload | null
  open: (payload: WalletConnectPairingHookPayload) => void
  close: () => void
}

export const useWalletConnectPairingStore = create<WalletConnectPairingState>((set) => ({
  isOpen: false,
  payload: null,
  open: (payload) => set({ isOpen: true, payload }),
  close: () => set({ isOpen: false, payload: null }),
}))
