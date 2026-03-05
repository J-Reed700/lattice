import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: true,
    hmr: {
      protocol: 'ws',
      host: 'localhost',
      port: 5173
    }
  },
  envPrefix: ['VITE_', 'TAURI_'],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './websrc'),
      // Keep PDF.js API + worker on the exact same package instance/version.
      // react-pdf currently depends on its own pdfjs-dist copy.
      'pdfjs-dist': path.resolve(__dirname, './node_modules/react-pdf/node_modules/pdfjs-dist'),
    },
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'react-vendor': ['react', 'react-dom', 'zustand'],
          'pdf-viewer': ['pdfjs-dist', 'react-pdf'],
          'tiptap': ['@tiptap/react', '@tiptap/starter-kit', 'tiptap-markdown', 'lowlight'],
          'syntax-highlighter': ['react-syntax-highlighter'],
          'ui-components': ['lucide-react', 'date-fns', 'cmdk'],
          'radix-ui': ['@radix-ui/react-tooltip'],
          'tauri': ['@tauri-apps/api', '@tauri-apps/plugin-dialog', '@tauri-apps/plugin-fs', '@tauri-apps/plugin-shell'],
          'virtualization': ['@tanstack/react-virtual'],
        },
      },
    },
    chunkSizeWarningLimit: 500,
  },
})
