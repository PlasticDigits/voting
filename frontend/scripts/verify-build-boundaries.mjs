import { spawnSync } from 'node:child_process'
const baseline = Object.fromEntries(
  Object.entries(process.env).filter(([key]) => !key.startsWith('VITE_')),
)
const clean = {
  VITE_OPERATOR_VOTING_URL: 'https://operator.vote.cl8y.com',
  VITE_WC_PROJECT_ID: 'public-build-test-id',
}
const cases = {
  missing_api: { VITE_OPERATOR_VOTING_URL: '' },
  insecure_api: { VITE_OPERATOR_VOTING_URL: 'http://api.example' },
  credentials_api: { VITE_OPERATOR_VOTING_URL: 'https://public:sentinel@api.example' },
  local_network: { VITE_NETWORK: 'local' },
  dev_chain: { VITE_EVM_CHAIN_ID: '31337' },
  simulated_wallet: { VITE_DEV_MODE: 'true' },
  browser_balance_rpc: { VITE_BSC_RPC_URL: 'https://bsc.example' },
  legal_hatch: { VITE_PLAYWRIGHT_E2E: 'true' },
  missing_walletconnect: { VITE_WC_PROJECT_ID: '' },
}
let failures = 0
for (const mode of ['production', 'custom-release'])
  for (const [name, env] of Object.entries(cases)) {
    const result = spawnSync(
      process.execPath,
      ['node_modules/vite/bin/vite.js', 'build', '--mode', mode],
      { env: { ...baseline, ...clean, ...env }, encoding: 'utf8' },
    )
    const pass = result.status !== 0 && result.stderr.includes('failed to load config')
    if (!pass) failures++
    console.log(JSON.stringify({ mode, case: name, rejected: pass, status: result.status }))
  }
process.exitCode = failures ? 1 : 0
