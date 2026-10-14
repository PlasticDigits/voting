/**
 * Production / Coolify build guards for issue #7 (O3, O4) and #12 (WC project id).
 *
 * Used by `vite.config.ts` so `npm run build` fails closed the same way
 * `deploy/docker/frontend.Dockerfile` does. Keep this module free of
 * `import.meta.env` so Node (Vite config) and Vitest can both import it.
 */

export function productionEnvViolations(env: Record<string, string | undefined>): string[] {
  const violations: string[] = []
  try {
    const api = new URL(env.VITE_OPERATOR_VOTING_URL ?? '')
    if (
      api.protocol !== 'https:' ||
      api.username ||
      api.password ||
      api.search ||
      api.hash ||
      /^(localhost|127\.|\[?::1\]?$)/.test(api.hostname)
    ) {
      violations.push(
        'VITE_OPERATOR_VOTING_URL must be a public HTTPS URL without credentials, query or fragment.',
      )
    }
  } catch {
    violations.push('VITE_OPERATOR_VOTING_URL is required and must be an absolute HTTPS URL.')
  }
  if (env.VITE_NETWORK && env.VITE_NETWORK !== 'mainnet')
    violations.push('VITE_NETWORK must be mainnet for production builds.')
  if (env.VITE_EVM_CHAIN_ID && env.VITE_EVM_CHAIN_ID !== '56')
    violations.push('VITE_EVM_CHAIN_ID must be 56 for production builds.')
  if (env.VITE_TERRA_CHAIN_ID && env.VITE_TERRA_CHAIN_ID !== 'columbus-5')
    violations.push('VITE_TERRA_CHAIN_ID must be columbus-5 for production builds.')
  for (const key of [
    'VITE_DEV_MODE',
    'VITE_DEV_WALLET',
    'VITE_SIMULATED_WALLET',
    'VITE_USE_DEV_WALLET',
  ]) {
    if (env[key]?.trim()) violations.push(`${key} must be unset for production builds.`)
  }

  if (env.VITE_PLAYWRIGHT_E2E?.trim()) {
    violations.push('VITE_PLAYWRIGHT_E2E must be unset for production builds (O3).')
  }
  if (env.VITE_DEV_MNEMONIC?.trim()) {
    violations.push('VITE_DEV_MNEMONIC must be unset for production builds (O3).')
  }
  if (env.VITE_REGISTER_POLL_TIMEOUT_MS?.trim()) {
    violations.push(
      'VITE_REGISTER_POLL_TIMEOUT_MS must be unset for production builds (Playwright-only; default 120s).',
    )
  }
  if (!env.VITE_WC_PROJECT_ID?.trim()) {
    violations.push(
      'VITE_WC_PROJECT_ID is required for production builds (issue #12 / DEX #378 / M-10).',
    )
  }

  for (const [key, value] of Object.entries(env)) {
    if (!value?.trim() || !key.startsWith('VITE_')) continue
    if (/BSC/i.test(key) && /RPC/i.test(key)) {
      violations.push(
        `${key} must not be set (O4: ledger / operator-voting own BSC eth_call and eth_getLogs).`,
      )
    }
  }

  return violations
}

export function assertProductionVotingEnv(env: Record<string, string | undefined>): void {
  const violations = productionEnvViolations(env)
  if (violations.length > 0) {
    throw new Error(violations.join('\n'))
  }
}
