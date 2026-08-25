import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'
import { assertProductionVotingEnv } from './src/utils/prodEnvGuards'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, __dirname, 'VITE_')
  if (mode === 'production') {
    assertProductionVotingEnv(env)
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
