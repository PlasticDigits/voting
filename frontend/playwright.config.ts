import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  workers: 5,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  use: {
    baseURL: 'http://127.0.0.1:5176',
    trace: 'on-first-retry',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 5176 --strictPort',
    url: 'http://127.0.0.1:5176',
    reuseExistingServer: false,
    timeout: 120_000,
    env: {
      ...process.env,
      VITE_PLAYWRIGHT_E2E: 'true',
      VITE_DEV_MODE: 'true',
      VITE_NETWORK: 'local',
      VITE_OPERATOR_VOTING_URL: 'http://127.0.0.1:3999',
      VITE_DEV_MNEMONIC:
        process.env.VITE_DEV_MNEMONIC ||
        'notice oak worry limit wrap speak medal online prefer cluster roof addict wrist behave treat actual wasp year salad speed social layer crew genius',
    },
  },
})
