import { rememberKeplrNanoLedgerFlag, setSessionNanoLedger } from '@/services/terraclassic/keplrExtensionConfig'
import { applyStationKeplrShimSignDefaults } from '@/services/terraclassic/stationExtensionConfig'
import { getTerraChainSuggestion } from '@/services/terraclassic/terraChainSuggestion'
import {
  ensureStationLocalNetworkRegistered,
  shouldUseStationNativeLocalNetwork,
} from '@/services/terraclassic/stationNativeNetwork'
import { getKeplrLikeExtension } from '@/services/terraclassic/keplrLikeExtension'
import { isBrowserWalletExtensionDetected } from '@/services/terraclassic/walletExtensionInstall'
import { effectiveGasPriceUluna } from '@/utils/constants'
import {
  buildWrongNetworkConnectError,
  isWalletExtensionNotInstalledError,
  isWalletWrongNetworkError,
} from '@/utils/walletNetworkError'
import {
  ConnectedWallet,
  CosmostationController,
  GalaxyStationController,
  KeplrController,
  LUNCDashController,
  StationController,
  WalletController,
  WalletName,
  WalletType,
} from '@goblinhunt/cosmes/wallet'
import { NETWORKS, DEFAULT_NETWORK } from '@/utils/constants'
import {
  isWalletConnectCancelledError,
  isWalletConnectTimeoutError,
  raceWithAbortAndTimeout,
  WALLETCONNECT_CONNECT_TIMEOUT_MS,
} from '@/utils/walletConnectSession'

async function suggestChainToExtension(walletName: WalletName): Promise<void> {
  const ext = getKeplrLikeExtension(walletName)
  if (ext?.experimentalSuggestChain) {
    await ext.experimentalSuggestChain(getTerraChainSuggestion())
  }
}

const networkConfig = NETWORKS[DEFAULT_NETWORK].terra
const TERRA_CLASSIC_CHAIN_ID = networkConfig.chainId
/** Production builds require VITE_WC_PROJECT_ID (vite.config.ts guard — GitLab #378 / M-10). */
const WC_PROJECT_ID = import.meta.env.VITE_WC_PROJECT_ID ?? ''

const GAS_PRICE = {
  amount: String(effectiveGasPriceUluna()),
  denom: 'uluna',
}

/** Persisted in Zustand / analytics; excludes sunset wallets (GitLab #159: Leap). */
export type TerraWalletBackend = 'station' | 'keplr' | 'luncdash' | 'galaxy' | 'cosmostation'

const STATION_CONTROLLER = new StationController()
const KEPLR_CONTROLLER = new KeplrController(WC_PROJECT_ID)
const LUNCDASH_CONTROLLER = new LUNCDashController()
const GALAXY_CONTROLLER = new GalaxyStationController(WC_PROJECT_ID)
const COSMOSTATION_CONTROLLER = new CosmostationController(WC_PROJECT_ID)

const CONTROLLERS: Partial<Record<WalletName, WalletController>> = {
  [WalletName.STATION]: STATION_CONTROLLER,
  [WalletName.KEPLR]: KEPLR_CONTROLLER,
  [WalletName.LUNCDASH]: LUNCDASH_CONTROLLER,
  [WalletName.GALAXYSTATION]: GALAXY_CONTROLLER,
  [WalletName.COSMOSTATION]: COSMOSTATION_CONTROLLER,
}

const WALLET_DISPLAY_NAMES: Record<string, string> = {
  [WalletName.STATION]: 'Station',
  [WalletName.KEPLR]: 'Keplr',
  [WalletName.LUNCDASH]: 'LuncDash',
  [WalletName.GALAXYSTATION]: 'Galaxy',
  [WalletName.COSMOSTATION]: 'Cosmostation',
}

const WALLET_TYPE_STRINGS: Record<string, TerraWalletBackend> = {
  [WalletName.STATION]: 'station',
  [WalletName.KEPLR]: 'keplr',
  [WalletName.LUNCDASH]: 'luncdash',
  [WalletName.GALAXYSTATION]: 'galaxy',
  [WalletName.COSMOSTATION]: 'cosmostation',
}

const connectedWallets: Map<string, ConnectedWallet> = new Map()

type PendingWalletConnect = {
  walletName: WalletName
  abort: AbortController
}

let pendingWalletConnect: PendingWalletConnect | null = null

function disconnectWalletConnectTransport(controller: WalletController | undefined): void {
  if (!controller) return
  try {
    controller.disconnect([TERRA_CLASSIC_CHAIN_ID])
  } catch {
    /* pending WC may not have a session yet */
  }
  const wc = (controller as WalletController & { wc?: { disconnect?: () => void } }).wc
  try {
    wc?.disconnect?.()
  } catch {
    /* ignore */
  }
}

/** Abort in-flight WalletConnect `controller.connect()` (GitLab #554). */
export function abortPendingTerraWalletConnect(): void {
  const pending = pendingWalletConnect
  pendingWalletConnect = null
  pending?.abort.abort()
  if (pending) {
    disconnectWalletConnectTransport(CONTROLLERS[pending.walletName])
  }
}

export type ConnectTerraWalletOptions = {
  signal?: AbortSignal
  timeoutMs?: number
}

function getChainInfo() {
  return {
    chainId: TERRA_CLASSIC_CHAIN_ID,
    rpc: networkConfig.rpc,
    gasPrice: GAS_PRICE,
  }
}

export function isStationInstalled(): boolean {
  return isBrowserWalletExtensionDetected(WalletName.STATION)
}

export function isKeplrInstalled(): boolean {
  return isBrowserWalletExtensionDetected(WalletName.KEPLR)
}

export async function connectTerraWallet(
  walletName: WalletName = WalletName.STATION,
  walletType: WalletType = WalletType.EXTENSION,
  options?: ConnectTerraWalletOptions
): Promise<{
  address: string
  walletType: TerraWalletBackend
  connectionType: WalletType
}> {
  const controller = CONTROLLERS[walletName]
  if (!controller) {
    throw new Error(`Unsupported wallet: ${walletName}`)
  }

  try {
    const chainInfo = getChainInfo()
    console.log(`[Wallet] Connecting ${walletName} (${walletType}) to chain ${chainInfo.chainId}`, {
      rpc: chainInfo.rpc,
      gasPrice: chainInfo.gasPrice,
    })

    const SUGGEST_CHAIN_WALLETS: WalletName[] = [WalletName.KEPLR, WalletName.COSMOSTATION]
    const isStationExtension = walletName === WalletName.STATION && walletType === WalletType.EXTENSION
    const isStationLocalExtension = isStationExtension && DEFAULT_NETWORK === 'local'

    if (walletType === WalletType.EXTENSION) {
      if (isStationExtension) {
        applyStationKeplrShimSignDefaults()
      }
      if (isStationLocalExtension) {
        // New Station: keplr.experimentalSuggestChain rejects localterra (#207). Register via addNetwork first.
        if (shouldUseStationNativeLocalNetwork()) {
          await ensureStationLocalNetworkRegistered(networkConfig.lcd, TERRA_CLASSIC_CHAIN_ID)
        } else {
          try {
            await suggestChainToExtension(walletName)
          } catch (err: unknown) {
            console.warn(
              '[Wallet] Station experimentalSuggestChain failed; approve the chain update in Station or LocalTerra fees may stay too low (GitLab #127):',
              err
            )
          }
        }
      } else if (SUGGEST_CHAIN_WALLETS.includes(walletName) || isStationExtension) {
        try {
          await suggestChainToExtension(walletName)
        } catch (err: unknown) {
          if (isStationExtension) {
            console.warn(
              '[Wallet] Station experimentalSuggestChain failed; approve the Terra Classic chain update in Station (GitLab #208):',
              err
            )
          } else {
            throw err
          }
        }
      }
    }

    let wallets: Map<string, ConnectedWallet>
    const wcTimeoutMs =
      walletType === WalletType.WALLETCONNECT ? (options?.timeoutMs ?? WALLETCONNECT_CONNECT_TIMEOUT_MS) : undefined
    const abort = new AbortController()
    const onExternalAbort = () => abort.abort()
    if (options?.signal) {
      if (options.signal.aborted) {
        abort.abort()
      } else {
        options.signal.addEventListener('abort', onExternalAbort, { once: true })
      }
    }
    pendingWalletConnect = { walletName, abort }
    try {
      const connectPromise = controller.connect(walletType, [chainInfo])
      wallets =
        wcTimeoutMs != null
          ? await raceWithAbortAndTimeout(connectPromise, {
              timeoutMs: wcTimeoutMs,
              signal: abort.signal,
              onTimeout: () => disconnectWalletConnectTransport(controller),
            })
          : await connectPromise
    } catch (connectError: unknown) {
      if (isWalletConnectCancelledError(connectError) || isWalletConnectTimeoutError(connectError)) {
        throw connectError
      }
      console.error(`[Wallet] Controller.connect() threw an error:`, connectError)
      const errorMessage = connectError instanceof Error ? connectError.message : String(connectError)
      const errorStack = connectError instanceof Error ? connectError.stack : undefined
      console.error(`[Wallet] Error details:`, { errorMessage, errorStack })
      throw connectError
    } finally {
      options?.signal?.removeEventListener('abort', onExternalAbort)
      if (pendingWalletConnect?.abort === abort) {
        pendingWalletConnect = null
      }
    }

    console.log(`[Wallet] Controller returned ${wallets.size} wallet(s)`, {
      walletName,
      walletType,
      chainIds: Array.from(wallets.keys()),
    })

    if (wallets.size === 0) {
      const isLuncDashWC = walletType === WalletType.WALLETCONNECT && walletName === WalletName.LUNCDASH
      const isStationWC = walletType === WalletType.WALLETCONNECT && walletName === WalletName.STATION

      if (isLuncDashWC || isStationWC) {
        const sessionKey = isLuncDashWC ? 'cosmes.wallet.luncdash.wcSession' : 'cosmes.wallet.station.wcSession'
        const walletDisplayName = isLuncDashWC ? 'LUNC Dash' : 'Station'

        const cachedSession = typeof window !== 'undefined' ? localStorage.getItem(sessionKey) : null

        console.log(`[${walletDisplayName}] Checking for cached WalletConnect session`, {
          sessionKey,
          hasCachedSession: !!cachedSession,
        })

        let session: { accounts?: string[]; chainId?: number; peerMeta?: unknown } | null = null
        if (cachedSession) {
          try {
            const parsed = JSON.parse(cachedSession)
            if (parsed && typeof parsed === 'object') {
              if (
                parsed.accounts &&
                Array.isArray(parsed.accounts) &&
                parsed.accounts.every((a: unknown) => typeof a === 'string')
              ) {
                session = parsed
              } else if (!parsed.accounts) {
                session = parsed
              }
            }
          } catch (parseError) {
            console.error(`[${walletDisplayName}] Failed to parse cached session JSON:`, parseError)
            session = null
          }
        }

        if (session) {
          console.log(`[${walletDisplayName}] Cached session found`, {
            hasAccounts: !!(session && session.accounts),
            accountCount: session?.accounts?.length || 0,
            accounts: session?.accounts,
            peerMeta: session?.peerMeta,
          })

          if (session.accounts && session.accounts.length > 0) {
            const address = session.accounts[0]
            console.log(
              `[${walletDisplayName}] WalletConnect succeeded but controller returned 0 wallets. Diagnosing...`,
              {
                address,
                chainId: chainInfo.chainId,
                rpc: chainInfo.rpc,
              }
            )

            const lcdUrl = networkConfig.lcd.replace(':443', '')
            const accountUrl = `${lcdUrl}/cosmos/auth/v1beta1/account_info/${address}`

            console.log(`[${walletDisplayName}] Manually fetching account info from LCD:`, accountUrl)

            try {
              const accountResponse = await fetch(accountUrl)
              const accountData = await accountResponse.json()

              console.log(`[${walletDisplayName}] Manual account fetch result:`, {
                status: accountResponse.status,
                ok: accountResponse.ok,
                data: accountData,
              })

              if (accountResponse.ok && accountData.info) {
                console.log(`[${walletDisplayName}] Account data retrieved successfully!`)

                const info = accountData.info
                const hasPubKey = !!info.pub_key

                console.log(`[${walletDisplayName}] Account details:`, {
                  address: info.address,
                  hasPubKey: hasPubKey,
                  pubKey: info.pub_key,
                  accountNumber: info.account_number,
                  sequence: info.sequence,
                })

                if (!hasPubKey) {
                  throw new Error(
                    `${walletDisplayName} WalletConnect succeeded, but your account (${address}) does not have a public key on-chain yet. ` +
                      `This happens when an account has received funds but never sent a transaction. ` +
                      `Please send any transaction from this wallet first (e.g., a small LUNC transfer to yourself), then try connecting again.`
                  )
                }

                console.log(
                  `[${walletDisplayName}] Account has pub key. The issue is likely that cosmes uses RPC instead of LCD for account queries.`
                )
                throw new Error(
                  `${walletDisplayName} WalletConnect succeeded and account has pub key, but the cosmes library failed to retrieve it. ` +
                    `This is likely because cosmes uses the RPC endpoint which may not support account queries on Terra Classic. ` +
                    `Address: ${address}. Please check browser console for more details.`
                )
              } else {
                throw new Error(
                  `${walletDisplayName} WalletConnect succeeded, but failed to fetch account info. ` +
                    `Address: ${address}. Status: ${accountResponse.status}. ` +
                    `Response: ${JSON.stringify(accountData)}`
                )
              }
            } catch (fetchError: unknown) {
              if (
                fetchError instanceof Error &&
                (fetchError.message.includes('WalletConnect succeeded') ||
                  fetchError.message.includes('does not have a public key'))
              ) {
                throw fetchError
              }

              const fetchErrorMessage = fetchError instanceof Error ? fetchError.message : String(fetchError)
              console.error(`[${walletDisplayName}] Failed to manually fetch account:`, fetchError)

              throw new Error(
                `${walletDisplayName} WalletConnect succeeded, but cannot reach LCD to verify account. ` +
                  `Address: ${address}. LCD: ${accountUrl}. ` +
                  `Error: ${fetchErrorMessage}`
              )
            }
          }
        }

        throw new Error(
          `${walletDisplayName} connection failed: Unable to retrieve wallet information. ` +
            'The mobile wallet may be connected, but the dapp could not verify the connection. ' +
            'Please check the browser console for detailed logs and try disconnecting and reconnecting.'
        )
      }

      if (walletName === WalletName.STATION && walletType === WalletType.EXTENSION) {
        if (isStationLocalExtension) {
          throw new Error(
            'Station could not connect to LocalTerra. Approve adding the LocalTerra network when Station prompts, ' +
              `ensure the extension is unlocked, and confirm LCD ${networkConfig.lcd} is reachable.`
          )
        }
        throw new Error(buildWrongNetworkConnectError('Station'))
      }
      if (walletType === WalletType.EXTENSION && isBrowserWalletExtensionDetected(walletName)) {
        const label = WALLET_DISPLAY_NAMES[walletName] ?? 'Wallet'
        throw new Error(buildWrongNetworkConnectError(label))
      }
      throw new Error('No wallets connected')
    }

    const wallet = wallets.get(TERRA_CLASSIC_CHAIN_ID)
    if (!wallet) {
      if (walletType === WalletType.EXTENSION && isBrowserWalletExtensionDetected(walletName)) {
        const label = WALLET_DISPLAY_NAMES[walletName] ?? 'Wallet'
        throw new Error(buildWrongNetworkConnectError(label))
      }
      throw new Error(`Failed to connect to Terra Classic chain (${TERRA_CLASSIC_CHAIN_ID})`)
    }

    connectedWallets.set(TERRA_CLASSIC_CHAIN_ID, wallet)

    if (walletName === WalletName.KEPLR && walletType === WalletType.EXTENSION) {
      await rememberKeplrNanoLedgerFlag(wallet)
    }

    // Second suggest / network refresh after enable + wallet init (GitLab #127, #208).
    if (isStationExtension) {
      if (isStationLocalExtension && shouldUseStationNativeLocalNetwork()) {
        try {
          await ensureStationLocalNetworkRegistered(networkConfig.lcd, TERRA_CLASSIC_CHAIN_ID)
        } catch (err: unknown) {
          console.warn(
            '[Wallet] Post-connect Station addNetwork refresh failed; LocalTerra fees may stay too low (GitLab #127):',
            err
          )
        }
      } else if (isStationLocalExtension && !shouldUseStationNativeLocalNetwork()) {
        try {
          await suggestChainToExtension(walletName)
        } catch (err: unknown) {
          console.warn(
            '[Wallet] Post-connect Station experimentalSuggestChain failed; fees may still be low (GitLab #127):',
            err
          )
        }
      } else if (!isStationLocalExtension) {
        try {
          await suggestChainToExtension(walletName)
        } catch (err: unknown) {
          console.warn('[Wallet] Post-connect Station experimentalSuggestChain failed (GitLab #208):', err)
        }
      }
      applyStationKeplrShimSignDefaults()
    }

    const walletTypeStr: TerraWalletBackend = WALLET_TYPE_STRINGS[walletName] ?? 'keplr'

    return {
      address: wallet.address,
      walletType: walletTypeStr,
      connectionType: walletType,
    }
  } catch (error: unknown) {
    if (isWalletConnectCancelledError(error) || isWalletConnectTimeoutError(error)) {
      throw error
    }
    const errorMessage = error instanceof Error ? error.message : 'Unknown error'

    if (isWalletWrongNetworkError(errorMessage)) {
      const label = WALLET_DISPLAY_NAMES[walletName] ?? 'Wallet'
      throw new Error(buildWrongNetworkConnectError(label))
    }

    if (walletName === WalletName.KEPLR) {
      if (isWalletExtensionNotInstalledError(errorMessage) || /keplr extension is not installed/i.test(errorMessage)) {
        throw new Error('Keplr wallet is not installed. Please install the Keplr extension.')
      }
      if (errorMessage.includes('chain') && errorMessage.includes('not found')) {
        throw new Error(buildWrongNetworkConnectError('Keplr'))
      }
    }

    if (walletName === WalletName.STATION) {
      if (
        isWalletExtensionNotInstalledError(errorMessage) ||
        /station extension is not installed/i.test(errorMessage)
      ) {
        throw new Error('Station wallet is not installed. Please install the Station extension.')
      }
    }

    if (errorMessage.includes('User rejected') || errorMessage.includes('rejected')) {
      throw new Error('Connection rejected by user')
    }

    const walletDisplayName = WALLET_DISPLAY_NAMES[walletName] ?? 'wallet'

    throw new Error(`Failed to connect ${walletDisplayName}: ${errorMessage}`)
  }
}

export async function disconnectTerraWallet(): Promise<void> {
  const wallet = connectedWallets.get(TERRA_CLASSIC_CHAIN_ID)
  if (wallet) {
    const controller = CONTROLLERS[wallet.id]
    if (controller) {
      controller.disconnect([TERRA_CLASSIC_CHAIN_ID])
    }
    connectedWallets.delete(TERRA_CLASSIC_CHAIN_ID)
    setSessionNanoLedger(false)
  }
}

export function registerConnectedWallet(wallet: ConnectedWallet): void {
  connectedWallets.set(TERRA_CLASSIC_CHAIN_ID, wallet)
}

export function getConnectedWallet(): ConnectedWallet | null {
  return connectedWallets.get(TERRA_CLASSIC_CHAIN_ID) || null
}

export async function getCurrentTerraAddress(): Promise<string | null> {
  const wallet = connectedWallets.get(TERRA_CLASSIC_CHAIN_ID)
  if (wallet) {
    return wallet.address
  }

  try {
    if (isStationInstalled()) {
      const result = await connectTerraWallet(WalletName.STATION, WalletType.EXTENSION)
      return result.address
    } else if (isKeplrInstalled()) {
      const result = await connectTerraWallet(WalletName.KEPLR, WalletType.EXTENSION)
      return result.address
    }
  } catch {
    // Ignore errors on auto-connect
  }

  return null
}

export async function isTerraWalletConnected(): Promise<boolean> {
  const address = await getCurrentTerraAddress()
  return address !== null
}
