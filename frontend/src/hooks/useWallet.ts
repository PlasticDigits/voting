import { create } from 'zustand'
import {
  abortPendingTerraWalletConnect,
  connectTerraWallet,
  disconnectTerraWallet,
  registerConnectedWallet,
} from '@/services/terraclassic/wallet'
import { createDevTerraWallet } from '@/services/terraclassic/devWallet'
import { DEV_MODE } from '@/utils/constants'
import { humanizeUserFacingError } from '@/utils/humanizeUserFacingError'
import { isWalletConnectCancelledError, WALLETCONNECT_CONNECT_TIMEOUT_MS } from '@/utils/walletConnectSession'
import { useWalletConnectPairingStore } from '@/hooks/useWalletConnectPairingStore'
import { useEvmWalletStore } from '@/stores/evmWallet'
import { WalletName, WalletType } from '@goblinhunt/cosmes/wallet'

const WALLET_STORAGE_KEY = 'cl8y_wallet_connection'
/** Persists simulated dev wallet across full page loads (Playwright `page.goto`, refresh). */
const DEV_SIM_STORAGE_KEY = 'cl8y_dev_sim'

/** Bumps on cancel so a late WalletConnect session cannot attach (GitLab #554). */
let connectAttemptId = 0

interface WalletState {
  address: string | null
  walletType: string | null
  isConnecting: boolean
  error: string | null
  walletModalOpen: boolean
  setWalletModalOpen: (open: boolean) => void
  openWalletModal: () => void
  closeWalletModal: () => void
  connect: (walletName: WalletName, walletType: WalletType) => Promise<void>
  connectDev: () => void
  disconnect: () => Promise<void>
  /** Clears Terra + EVM connecting, aborts pending WalletConnect, closes pairing (GitLab #554 / voting #12). */
  cancelConnection: () => void
}

export const useWalletStore = create<WalletState>((set, get) => ({
  address: null,
  walletType: null,
  isConnecting: false,
  error: null,
  walletModalOpen: false,
  setWalletModalOpen: (open) => set({ walletModalOpen: open }),
  openWalletModal: () => set({ walletModalOpen: true }),
  closeWalletModal: () => {
    if (get().isConnecting || useEvmWalletStore.getState().connecting) {
      get().cancelConnection()
      return
    }
    set({ walletModalOpen: false })
  },
  connect: async (walletName, walletType) => {
    const attempt = ++connectAttemptId
    set({ isConnecting: true, error: null })
    try {
      await useEvmWalletStore.getState().disconnect()
      const timeoutMs = walletType === WalletType.WALLETCONNECT ? WALLETCONNECT_CONNECT_TIMEOUT_MS : undefined
      const result =
        timeoutMs != null
          ? await connectTerraWallet(walletName, walletType, { timeoutMs })
          : await connectTerraWallet(walletName, walletType)
      if (attempt !== connectAttemptId) {
        try {
          await disconnectTerraWallet()
        } catch {
          /* late session after cancel must not stick */
        }
        return
      }
      try {
        localStorage.setItem(WALLET_STORAGE_KEY, JSON.stringify({ walletName, walletType }))
      } catch {
        /* storage unavailable */
      }
      set({ address: result.address, walletType: result.walletType, isConnecting: false, walletModalOpen: false })
    } catch (err) {
      if (attempt !== connectAttemptId || isWalletConnectCancelledError(err)) {
        if (attempt === connectAttemptId) {
          set({ isConnecting: false })
        }
        return
      }
      const raw = err instanceof Error ? err.message : 'Connection failed'
      useWalletConnectPairingStore.getState().close()
      set({ error: humanizeUserFacingError(raw), isConnecting: false })
      throw err
    }
  },
  connectDev: () => {
    if (!DEV_MODE) return
    void useEvmWalletStore.getState().disconnect()
    try {
      localStorage.setItem(DEV_SIM_STORAGE_KEY, '1')
    } catch {
      /* storage unavailable */
    }
    const devWallet = createDevTerraWallet()
    registerConnectedWallet(devWallet)
    set({ address: devWallet.address, walletType: 'simulated', error: null, walletModalOpen: false })
  },
  disconnect: async () => {
    await disconnectTerraWallet()
    try {
      localStorage.removeItem(WALLET_STORAGE_KEY)
      localStorage.removeItem(DEV_SIM_STORAGE_KEY)
    } catch {
      /* storage unavailable */
    }
    set({ address: null, walletType: null })
  },
  cancelConnection: () => {
    connectAttemptId += 1
    abortPendingTerraWalletConnect()
    useWalletConnectPairingStore.getState().close()
    useEvmWalletStore.getState().cancelConnection()
    set({ isConnecting: false, error: null, walletModalOpen: false })
  },
}))

const VALID_WALLET_NAMES = new Set(Object.values(WalletName as Record<string, string>))
const VALID_WALLET_TYPES = new Set(Object.values(WalletType as Record<string, string>))

const RECONNECT_MAX_RETRIES = 3
const RECONNECT_BASE_DELAY_MS = 600

function isPermanentError(err: unknown): boolean {
  if (!(err instanceof Error)) return false
  const msg = err.message.toLowerCase()
  return msg.includes('rejected') || msg.includes('not installed') || msg.includes('unsupported')
}

async function attemptAutoReconnect(): Promise<void> {
  let saved: string | null
  try {
    saved = localStorage.getItem(WALLET_STORAGE_KEY)
  } catch {
    return
  }
  if (!saved) return

  let parsed: { walletName?: string; walletType?: string }
  try {
    parsed = JSON.parse(saved)
  } catch {
    localStorage.removeItem(WALLET_STORAGE_KEY)
    return
  }

  if (
    !parsed ||
    typeof parsed !== 'object' ||
    typeof parsed.walletName !== 'string' ||
    typeof parsed.walletType !== 'string'
  ) {
    try {
      localStorage.removeItem(WALLET_STORAGE_KEY)
    } catch {
      /* */
    }
    return
  }

  if (parsed.walletName === WalletName.LEAP) {
    try {
      localStorage.removeItem(WALLET_STORAGE_KEY)
    } catch {
      /* */
    }
    return
  }

  if (!VALID_WALLET_NAMES.has(parsed.walletName) || !VALID_WALLET_TYPES.has(parsed.walletType)) {
    try {
      localStorage.removeItem(WALLET_STORAGE_KEY)
    } catch {
      /* */
    }
    return
  }

  const { walletName, walletType } = parsed as { walletName: WalletName; walletType: WalletType }

  for (let attempt = 0; attempt < RECONNECT_MAX_RETRIES; attempt++) {
    try {
      await useWalletStore.getState().connect(walletName, walletType)
      return
    } catch (err) {
      if (isPermanentError(err)) {
        try {
          localStorage.removeItem(WALLET_STORAGE_KEY)
        } catch {
          /* */
        }
        return
      }
      if (attempt < RECONNECT_MAX_RETRIES - 1) {
        await new Promise((r) => setTimeout(r, RECONNECT_BASE_DELAY_MS * (attempt + 1)))
      }
    }
  }
}

function restoreDevSimIfNeeded(): void {
  if (!DEV_MODE) {
    try {
      localStorage.removeItem(DEV_SIM_STORAGE_KEY)
    } catch {
      /* storage unavailable */
    }
    return
  }
  try {
    if (localStorage.getItem(WALLET_STORAGE_KEY)) return
    if (localStorage.getItem(DEV_SIM_STORAGE_KEY) !== '1') return
    useWalletStore.getState().connectDev()
  } catch {
    /* storage unavailable */
  }
}

if (typeof window !== 'undefined') {
  const reconnect = () => {
    void attemptAutoReconnect().finally(() => {
      restoreDevSimIfNeeded()
    })
  }

  if (document.readyState === 'complete') {
    reconnect()
  } else {
    window.addEventListener('load', reconnect, { once: true })
  }
}
