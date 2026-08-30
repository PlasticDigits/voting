import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { assertProductionVotingEnv, productionEnvViolations } from '@/utils/prodEnvGuards'

const deployDocker = resolve(__dirname, '../../../../deploy/docker')

describe('production env guards (O3 / O4)', () => {
  it('allows a clean Coolify-style env', () => {
    expect(
      productionEnvViolations({
        VITE_OPERATOR_VOTING_URL: 'https://voting-api.example',
        VITE_LEGAL_PROPERTY: 'vote.cl8y.com',
      }),
    ).toEqual([])
    expect(() =>
      assertProductionVotingEnv({
        VITE_OPERATOR_VOTING_URL: 'https://voting-api.example',
      }),
    ).not.toThrow()
  })

  it('rejects Legal hatch, mnemonic, and any VITE_* BSC RPC', () => {
    expect(productionEnvViolations({ VITE_PLAYWRIGHT_E2E: '1' }).length).toBeGreaterThan(0)
    expect(productionEnvViolations({ VITE_DEV_MNEMONIC: 'word '.repeat(24) }).length).toBeGreaterThan(
      0,
    )
    expect(productionEnvViolations({ VITE_BSC_RPC_URL: 'https://bsc.example' })).toEqual(
      expect.arrayContaining([expect.stringContaining('VITE_BSC_RPC_URL')]),
    )
    expect(productionEnvViolations({ VITE_BSC_JSON_RPC: 'https://bsc.example' }).length).toBeGreaterThan(
      0,
    )
    expect(() => assertProductionVotingEnv({ VITE_BSC_RPC: 'https://x' })).toThrow(/O4/)
    expect(productionEnvViolations({ VITE_REGISTER_POLL_TIMEOUT_MS: '4000' }).length).toBeGreaterThan(
      0,
    )
  })

  it('Coolify frontend image fail-closes hatch/RPC and ships explicit CSP', () => {
    const dockerfile = readFileSync(resolve(deployDocker, 'frontend.Dockerfile'), 'utf8')
    expect(dockerfile).toContain('test -z "$VITE_PLAYWRIGHT_E2E"')
    expect(dockerfile).toContain('test -z "$VITE_DEV_MNEMONIC"')
    expect(dockerfile).toContain('VITE_BSC_RPC_URL')

    const headers = readFileSync(resolve(deployDocker, 'frontend.security-headers.conf'), 'utf8')
    expect(headers).toContain('Content-Security-Policy')
    expect(headers).toContain('__OPERATOR_ORIGIN__')
    expect(headers).toContain('__LEGAL_API_ORIGIN__')
    const csp = headers.match(/Content-Security-Policy "([^"]+)"/)?.[1] ?? ''
    const connect = csp.split(';').find((d) => d.trim().startsWith('connect-src')) ?? ''
    expect(connect.split(/\s+/)).not.toContain('https:')
  })

  it('Coolify static nginx snippet stamps CSP (no blanket https:) for the live 1.31.x path', () => {
    const snippet = readFileSync(resolve(deployDocker, '../coolify-frontend.nginx.conf'), 'utf8')
    expect(snippet).toContain('try_files $uri /index.html')
    expect(snippet).toContain('X-Frame-Options DENY')
    expect(snippet).toContain('Content-Security-Policy')
    expect(snippet).toContain('https://operator.vote.cl8y.com')
    expect(snippet).toContain('https://api.terms.cl8y.com')
    expect(snippet).toContain('https://terms.cl8y.com')
    expect(snippet).not.toMatch(/add_header Cache-Control/)
    const csp = snippet.match(/Content-Security-Policy "([^"]+)"/)?.[1] ?? ''
    const connect = csp.split(';').find((d) => d.trim().startsWith('connect-src')) ?? ''
    expect(connect.split(/\s+/)).not.toContain('https:')
    expect(connect).toContain("connect-src")
  })
})
