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
    watch: {
      ignored: ['**/e2e-results/**', '**/src-tauri/target/**'],
    },
    hmr: {
      protocol: 'ws',
      host: 'localhost',
      port: 5173
    }
  },
  envPrefix: ['VITE_', 'TAURI_'],
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, './src'),
      // Keep PDF.js API + worker on the exact same package instance/version.
      // react-pdf currently depends on its own pdfjs-dist copy.
      'pdfjs-dist': path.resolve(import.meta.dirname, './node_modules/react-pdf/node_modules/pdfjs-dist'),
    },
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        // Vite 8 uses Rolldown, whose `manualChunks` accepts a function.
        // Keep heavyweight libraries isolated and every emitted JS chunk
        // below the 500 kB warning threshold.
        manualChunks(id) {
          const marker = '/node_modules/';
          const markerIndex = id.lastIndexOf(marker);
          if (markerIndex === -1) return undefined;

          const modulePath = id.slice(markerIndex + marker.length);
          const parts = modulePath.split('/');
          const packageName = modulePath.startsWith('@')
            ? `${parts[0]}/${parts[1]}`
            : parts[0];

          if (['react', 'react-dom', 'zustand'].includes(packageName)) return 'react-vendor';
          if (packageName === '@tanstack/react-query') return 'query-vendor';
          if (packageName === 'react-router') return 'router-vendor';
          if (packageName === 'framer-motion') return 'animation-vendor';
          if (packageName === 'pdfjs-dist' || packageName === 'react-pdf') return 'pdf-viewer';
          if (packageName === '@tiptap/pm' || packageName.startsWith('prosemirror-')) return 'tiptap-pm';
          if (packageName.startsWith('@tiptap/extension-') || packageName === 'tiptap-markdown') {
            return 'tiptap-extensions';
          }
          if (packageName === '@tiptap/react') return 'tiptap-react';
          if (packageName.startsWith('@tiptap/')) return 'tiptap-core';
          if (packageName === 'lowlight' || packageName === 'highlight.js') return 'syntax-languages';
          if (packageName === 'mammoth') return 'docx-renderer';
          if (packageName === 'dompurify') return 'sanitizer';
          if (packageName === 'react-day-picker') return 'calendar';
          if (['lucide-react', 'date-fns', 'cmdk'].includes(packageName)) return 'ui-components';
          if (packageName.startsWith('@radix-ui/')) return 'radix-ui';
          if (packageName.startsWith('@tauri-apps/')) return 'tauri';
          if (packageName === '@tanstack/react-virtual') return 'virtualization';
          return undefined;
        },
      },
    },
    chunkSizeWarningLimit: 500,
  },
})
