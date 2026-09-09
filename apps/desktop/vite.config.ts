import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Порт зафиксирован: его же ждёт devUrl в tauri.conf.json.
const DEV_PORT = 1420

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    watch: {
      // Пересборка фронтенда не должна запускаться из-за артефактов Rust.
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // Tauri поставляет собственный WebView: Edge WebView2 на Windows и WKWebView
    // на macOS 13+. Оба понимают ES2022, поэтому даунлевелить нечего.
    target: 'es2022',
    sourcemap: process.env.NODE_ENV !== 'production',
  },
})
