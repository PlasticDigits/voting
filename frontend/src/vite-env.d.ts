/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_OPERATOR_VOTING_URL?: string
  readonly VITE_LEGAL_PROPERTY?: string
  readonly VITE_LEGAL_API_BASE_URL?: string
  readonly VITE_LEGAL_TERMS_BASE_URL?: string
  readonly VITE_LEGAL_REDIRECT_ALLOWLIST?: string
  readonly VITE_WC_PROJECT_ID?: string
  readonly VITE_NETWORK?: string
  readonly VITE_TERRA_LCD_URL?: string
  readonly VITE_TERRA_RPC_URL?: string
  readonly VITE_DEV_MODE?: string
  readonly VITE_DEV_MNEMONIC?: string
  readonly VITE_PLAYWRIGHT_E2E?: string
  readonly VITE_GAS_PRICE_ULUNA?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

type KeplrSignArbitraryResponse = {
  signature: string
  pub_key: {
    type: string
    value: string
  }
}

interface Window {
  Buffer: typeof Buffer
  keplr?: {
    enable: (chainId: string) => Promise<void>
    experimentalSuggestChain: (chainInfo: Record<string, unknown>) => Promise<void>
    getKey: (chainId: string) => Promise<{
      name: string
      bech32Address: string
      pubKey: Uint8Array
      isNanoLedger: boolean
    }>
    getOfflineSigner: (chainId: string) => unknown
    signArbitrary: (
      chainId: string,
      signer: string,
      data: string | Uint8Array
    ) => Promise<KeplrSignArbitraryResponse>
  }
  station?: {
    connect: () => Promise<void>
    disconnect: () => Promise<void>
    addNetwork?: (network: {
      name: string
      chainID: string
      lcd: string
      prefix?: string
      coinType?: string
      baseAsset?: string
      gasAdjustment?: number
      gasPrices?: Record<string, number>
    }) => Promise<boolean>
    hasNetwork?: (network: { chainID: string; lcd: string }) => Promise<boolean>
    keplr?: {
      enable: (chainIds: string | string[]) => Promise<void>
      experimentalSuggestChain?: (chainInfo: Record<string, unknown>) => Promise<void>
      getKey: (chainId: string) => Promise<{
        name: string
        bech32Address: string
        pubKey: Uint8Array
        isNanoLedger: boolean
      }>
      getOfflineSigner: (chainId: string) => unknown
      defaultOptions?: { sign?: { preferNoSetFee?: boolean; preferNoSetMemo?: boolean } }
      signAmino?: (...args: unknown[]) => Promise<unknown>
      signDirect?: (...args: unknown[]) => Promise<unknown>
      signArbitrary?: Window['keplr'] extends { signArbitrary: infer T } ? T : never
    }
  }
}
