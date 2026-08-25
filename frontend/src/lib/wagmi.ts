import { http, createConfig } from 'wagmi'
import { bsc } from 'wagmi/chains'
import { coinbaseWallet, mock, walletConnect } from 'wagmi/connectors'
import { DEV_MODE, WC_PROJECT_ID } from '@/utils/constants'

const anvil = {
  id: 31337,
  name: 'Anvil',
  nativeCurrency: { decimals: 18, name: 'Ether', symbol: 'ETH' },
  rpcUrls: { default: { http: ['http://localhost:8545'] } },
  testnet: true,
} as const

const SIMULATED_EVM_ACCOUNTS = [
  '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266',
  '0x70997970C51812dc3A010C7d01b50e0d17dc79C8',
  '0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC',
] as const

const connectors = [
  ...(DEV_MODE
    ? [
        mock({
          accounts: SIMULATED_EVM_ACCOUNTS,
          features: { defaultConnected: false },
        }),
      ]
    : []),
  ...(WC_PROJECT_ID
    ? [
        walletConnect({
          projectId: WC_PROJECT_ID,
          metadata: {
            name: 'CL8Y Voting',
            description: 'Offchain advisory CL8Y snapshot voting',
            url: typeof window !== 'undefined' ? window.location.origin : 'https://vote.cl8y.com',
            icons: [`${typeof window !== 'undefined' ? window.location.origin : 'https://vote.cl8y.com'}/favicon.svg`],
          },
          showQrModal: true,
        }),
      ]
    : []),
  coinbaseWallet({ appName: 'CL8Y Voting' }),
]

const chains = DEV_MODE ? ([anvil, bsc] as const) : ([bsc] as const)

export const config = createConfig({
  chains,
  connectors,
  multiInjectedProviderDiscovery: true,
  transports: {
    [bsc.id]: http(),
    [anvil.id]: http('http://localhost:8545'),
  },
})

declare module 'wagmi' {
  interface Register {
    config: typeof config
  }
}
