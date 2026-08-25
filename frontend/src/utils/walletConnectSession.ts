/**
 * WalletConnect connect timeout / cancel (GitLab #554).
 *
 * Cosmes `controller.connect()` has no abort. The dApp races a bounded wait and
 * treats user dismiss as cancel so `isConnecting` cannot stick forever.
 */

export const WALLETCONNECT_CONNECT_TIMEOUT_MS = 90_000

export const WALLETCONNECT_TIMEOUT_MESSAGE = "Wallet didn't respond. Try again."

export const WALLETCONNECT_CANCELLED_MESSAGE = 'Connection cancelled'

export function isWalletConnectCancelledError(err: unknown): boolean {
  if (!(err instanceof Error)) return false
  return err.message === WALLETCONNECT_CANCELLED_MESSAGE || err.name === 'WalletConnectCancelledError'
}

export function isWalletConnectTimeoutError(err: unknown): boolean {
  if (!(err instanceof Error)) return false
  return err.message === WALLETCONNECT_TIMEOUT_MESSAGE || err.name === 'WalletConnectTimeoutError'
}

export function raceWithAbortAndTimeout<T>(
  promise: Promise<T>,
  options: { timeoutMs: number; signal: AbortSignal; onTimeout?: () => void }
): Promise<T> {
  const { timeoutMs, signal, onTimeout } = options
  return new Promise<T>((resolve, reject) => {
    if (signal.aborted) {
      reject(new Error(WALLETCONNECT_CANCELLED_MESSAGE))
      return
    }

    const timer = setTimeout(() => {
      onTimeout?.()
      reject(new Error(WALLETCONNECT_TIMEOUT_MESSAGE))
    }, timeoutMs)

    const onAbort = () => {
      clearTimeout(timer)
      reject(new Error(WALLETCONNECT_CANCELLED_MESSAGE))
    }
    signal.addEventListener('abort', onAbort, { once: true })

    promise.then(
      (value) => {
        clearTimeout(timer)
        signal.removeEventListener('abort', onAbort)
        resolve(value)
      },
      (err: unknown) => {
        clearTimeout(timer)
        signal.removeEventListener('abort', onAbort)
        reject(err)
      }
    )
  })
}
