import { defineConfig, devices } from '@playwright/test'
export default defineConfig({
  testDir: './protocol-e2e',
  workers: 1,
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5178' },
  webServer: [
    {
      command: 'npm run dev -- --host 127.0.0.1 --port 5178 --strictPort',
      url: 'http://127.0.0.1:5178',
      reuseExistingServer: false,
    },
    {
      command:
        'CARGO_BUILD_JOBS=2 cargo run -p operator-voting --example protocol_test_server --manifest-path ../Cargo.toml',
      url: 'http://127.0.0.1:3991/health',
      timeout: 180_000,
      reuseExistingServer: false,
    },
  ],
})
