import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The kit package is served from wherever a host puts it, so every asset path is relative.
export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: {
    dedupe: ['react', 'react-dom'],
  },
  worker: { format: 'es' },
  // Preserve the published editor's relative worker URLs and prebundle its CJS dependencies.
  optimizeDeps: {
    exclude: ['@kition/sdk'],
    include: ['@kition/tessera/ui', '@kition/sdk > use-sync-external-store/shim'],
  },
  server: {
    port: 5182,
    fs: { allow: [fileURLToPath(new URL('..', import.meta.url)), process.env.KITION_SDK ?? fileURLToPath(new URL('../../kition-sdk', import.meta.url)), process.env.KITION_TESSERA ?? fileURLToPath(new URL('../../kition-tessera', import.meta.url))] },
  },
  build: {
    outDir: 'dist',
    rollupOptions: { input: { index: fileURLToPath(new URL('index.html', import.meta.url)), dev: fileURLToPath(new URL('dev.html', import.meta.url)) } },
  },
})
