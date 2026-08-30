import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { assertProductionVotingEnv, productionEnvViolations } from '@/utils/prodEnvGuards'

const deployDocker = resolve(__dirname, '../../../../deploy/docker')

function cspDirective(csp: string, name: string): string {
  return csp.split(';').find((d) => d.trim().startsWith(name)) ?? ''
}

/** AppKit 1.7.8 hosts that `*.web3modal.com` / `wss://*.walletconnect.org` miss. */
const APPKIT_CONNECT_HOSTS = [
  'https://api.web3modal.org',
  'https://*.web3modal.org',
  'wss://relay.walletconnect.org',
  'https://relay.walletconnect.org',
  'https://rpc.walletconnect.org',
] as const

function expectAppKitCsp(csp: string) {
  const connect = cspDirective(csp, 'connect-src')
  expect(connect.split(/\s+/)).not.toContain('https:')
  for (const host of APPKIT_CONNECT_HOSTS) {
    expect(connect).toContain(host)
  }
  const frame = cspDirective(csp, 'frame-src')
  expect(frame).toContain('https://verify.walletconnect.com')
  expect(frame).toContain('https://secure.walletconnect.com')
  expect(frame.split(/\s+/)).not.toContain('*')
  const style = cspDirective(csp, 'style-src')
  expect(style).toContain('https://fonts.googleapis.com')
  expect(csp).toContain("frame-ancestors 'none'")
}

describe('production env guards (O3 / O4)', () => {
  it('allows a clean Coolify-style env', () => {
    expect(
      productionEnvViolations({
        VITE_OPERATOR_VOTING_URL: 'https://voting-api.example',
        VITE_LEGAL_PROPERTY: 'vote.cl8y.com',
        VITE_WC_PROJECT_ID: '00000000000000000000000000000000',
      }),
    ).toEqual([])
    expect(() =>
      assertProductionVotingEnv({
        VITE_OPERATOR_VOTING_URL: 'https://voting-api.example',
        VITE_WC_PROJECT_ID: '00000000000000000000000000000000',
      }),
    ).not.toThrow()
  })

  it('rejects missing VITE_WC_PROJECT_ID (issue #12)', () => {
    expect(productionEnvViolations({})).toEqual(
      expect.arrayContaining([expect.stringContaining('VITE_WC_PROJECT_ID')]),
    )
    expect(productionEnvViolations({ VITE_WC_PROJECT_ID: '   ' }).length).toBeGreaterThan(0)
    expect(() => assertProductionVotingEnv({ VITE_OPERATOR_VOTING_URL: 'https://x' })).toThrow(
      /VITE_WC_PROJECT_ID/,
    )
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
    expect(dockerfile).toContain('test -n "$VITE_WC_PROJECT_ID"')
    expect(dockerfile).toContain('COPY frontend/patches ./patches')
    expect(dockerfile).toContain('VITE_BSC_RPC_URL')

    const headers = readFileSync(resolve(deployDocker, 'frontend.security-headers.conf'), 'utf8')
    expect(headers).toContain('Content-Security-Policy')
    expect(headers).toContain('__OPERATOR_ORIGIN__')
    expect(headers).toContain('__LEGAL_API_ORIGIN__')
    const csp = headers.match(/Content-Security-Policy "([^"]+)"/)?.[1] ?? ''
    expectAppKitCsp(csp)
    expect(headers).toContain('https://walletconnect.luncdash.com')
    const img = cspDirective(csp, 'img-src')
    expect(img).toContain('https://*.walletconnect.com')
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
    expect(cspDirective(csp, 'connect-src')).toContain('connect-src')
    expectAppKitCsp(csp)
  })
})
