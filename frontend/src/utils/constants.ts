export const DEV_MODE = import.meta.env.VITE_DEV_MODE === 'true'

export const MIN_GAS_PRICE_ULUNA = 28.325
export const GAS_PRICE_ULUNA = import.meta.env.VITE_GAS_PRICE_ULUNA || String(MIN_GAS_PRICE_ULUNA)

export function effectiveGasPriceUluna(): number {
  const parsed = parseFloat(GAS_PRICE_ULUNA)
  const base = Number.isFinite(parsed) && parsed > 0 ? parsed : MIN_GAS_PRICE_ULUNA
  return Math.max(base, MIN_GAS_PRICE_ULUNA)
}

type NetworkConfig = {
  terra: {
    chainId: string
    lcd: string
    rpc: string
  }
}

export const NETWORKS: Record<string, NetworkConfig> = {
  local: {
    terra: {
      chainId: 'localterra',
      lcd: import.meta.env.VITE_TERRA_LCD_URL || 'http://localhost:1317',
      rpc: import.meta.env.VITE_TERRA_RPC_URL || 'http://localhost:26657',
    },
  },
  testnet: {
    terra: {
      chainId: 'rebel-2',
      lcd: 'https://terra-classic-lcd.publicnode.com',
      rpc: 'https://terra-classic-rpc.publicnode.com:443',
    },
  },
  mainnet: {
    terra: {
      chainId: 'columbus-5',
      lcd: import.meta.env.VITE_TERRA_LCD_URL || 'https://terra-classic-lcd.publicnode.com',
      rpc: import.meta.env.VITE_TERRA_RPC_URL || 'https://terra-classic-rpc.publicnode.com:443',
    },
  },
}

export const DEFAULT_NETWORK = (import.meta.env.VITE_NETWORK || 'mainnet') as keyof typeof NETWORKS

export const WC_PROJECT_ID = import.meta.env.VITE_WC_PROJECT_ID ?? ''

export const VOTING_APP = 'cl8y-voting'
export const TERRA_CHAIN = 'terra'
export const BSC_CHAIN = 'bsc'
export const TERRA_CHAIN_ID_MAINNET = 'columbus-5'
export const BSC_CHAIN_ID = '56'
export const SIGNATURE_TTL_SECS = 10 * 60
export const CL8Y_DECIMALS = 18
export const MIN_PROPOSAL_CL8Y = 1000
export const MIN_PROPOSAL_RAW = BigInt(MIN_PROPOSAL_CL8Y) * 10n ** BigInt(CL8Y_DECIMALS)

export function isValidTerraAddress(addr: string): boolean {
  return /^terra1[a-z0-9]{38,}$/.test(addr)
}

export function isValidEvmAddress(addr: string): boolean {
  return /^0x[a-fA-F0-9]{40}$/.test(addr)
}
