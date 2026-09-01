import { defineConfig, devices } from '@playwright/test'

// One webServer + 5 workers is the default (`npm run test:e2e`). Isolated
// agent shards set PW_PORT so they do not collide on 5176.
const port = Number(process.env.PW_PORT || 5176)

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  workers: 5,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: 'on-first-retry',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${port} --strictPort`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: false,
    timeout: 120_000,
    env: {
      ...process.env,
      VITE_PLAYWRIGHT_E2E: 'true',
      VITE_DEV_MODE: 'true',
      VITE_NETWORK: 'local',
      VITE_OPERATOR_VOTING_URL: 'http://127.0.0.1:3999',
      VITE_REGISTER_POLL_TIMEOUT_MS: '4000',
      VITE_DEV_MNEMONIC:
        process.env.VITE_DEV_MNEMONIC ||
        'notice oak worry limit wrap speak medal online prefer cluster roof addict wrist behave treat actual wasp year salad speed social layer crew genius',
    },
  },
})
