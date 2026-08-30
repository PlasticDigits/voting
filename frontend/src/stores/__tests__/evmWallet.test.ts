import { beforeEach, describe, expect, it, vi } from 'vitest'

const connect = vi.fn()
const disconnect = vi.fn()
const getAccount = vi.fn()
const getConnectors = vi.fn()

vi.mock('wagmi/actions', () => ({
  connect: (...args: unknown[]) => connect(...args),
  disconnect: (...args: unknown[]) => disconnect(...args),
  getAccount: (...args: unknown[]) => getAccount(...args),
  getConnectors: (...args: unknown[]) => getConnectors(...args),
}))

vi.mock('@/lib/wagmi', () => ({ config: { chains: [] } }))

describe('evmWallet cancelConnection (voting #12 / WC-M9)', () => {
  beforeEach(async () => {
    connect.mockReset()
    disconnect.mockReset()
    getAccount.mockReset()
    getConnectors.mockReset()
    getAccount.mockReturnValue({})
    getConnectors.mockReturnValue([])
    const { useEvmWalletStore } = await import('../evmWallet')
    useEvmWalletStore.setState({
      connected: false,
      connecting: false,
      address: null,
      connectorId: null,
      chainId: null,
      error: null,
    })
  })

  it('clears connecting and ignores a late wagmi session', async () => {
    let resolveConnect: ((value: { accounts: string[]; chainId: number }) => void) | undefined
    connect.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveConnect = resolve
        }),
    )
    getConnectors.mockReturnValue([{ id: 'walletConnect', uid: 'wc-1', type: 'walletConnect' }])

    const { useEvmWalletStore } = await import('../evmWallet')
    const pending = useEvmWalletStore.getState().connect('walletConnect')
    expect(useEvmWalletStore.getState().connecting).toBe(true)

    useEvmWalletStore.getState().cancelConnection()
    expect(useEvmWalletStore.getState().connecting).toBe(false)

    resolveConnect?.({ accounts: ['0xabc'], chainId: 56 })
    await pending

    expect(useEvmWalletStore.getState().address).toBeNull()
    expect(useEvmWalletStore.getState().connected).toBe(false)
    expect(disconnect).toHaveBeenCalled()
  })
})
