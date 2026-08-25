import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, __dirname, 'VITE_')
  if (mode === 'production') {
    if (env.VITE_DEV_MNEMONIC?.trim() && env.VITE_ALLOW_DEV_MNEMONIC !== 'local-only') {
      throw new Error('VITE_DEV_MNEMONIC must not be set for production builds.')
    }
    if (env.VITE_PLAYWRIGHT_E2E === 'true') {
      throw new Error('VITE_PLAYWRIGHT_E2E must not be enabled for production builds.')
    }
  }

  return {
    plugins: [react()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
        buffer: 'buffer',
      },
    },
    define: {
      global: 'globalThis',
    },
    server: {
      port: 5176,
      host: '127.0.0.1',
      strictPort: true,
    },
    optimizeDeps: {
      esbuildOptions: {
        define: { global: 'globalThis' },
      },
    },
  }
})
