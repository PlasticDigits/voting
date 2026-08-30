/**
 * Production / Coolify build guards for issue #7 (O3, O4).
 *
 * Used by `vite.config.ts` so `npm run build` fails closed the same way
 * `deploy/docker/frontend.Dockerfile` does. Keep this module free of
 * `import.meta.env` so Node (Vite config) and Vitest can both import it.
 */

export function productionEnvViolations(
  env: Record<string, string | undefined>,
): string[] {
  const violations: string[] = []

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
