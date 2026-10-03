import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { createReadStream, cpSync, mkdirSync, statSync } from 'node:fs'
import path from 'path'
import type { Plugin } from 'vite'

const excalidrawAssets = path.resolve(
  import.meta.dirname,
  'node_modules/@excalidraw/excalidraw/dist/prod',
)

/**
 * Excalidraw's runtime font loader expects its stylesheet and `fonts/` tree to
 * keep their original relative paths. Serve those assets in development and
 * copy them beside every production renderer build so the local-first editor
 * never falls back to its public CDN. The editor injects this stylesheet only
 * when Canvas opens, avoiding a second Vite-hashed copy of the font family.
 */
function packagedExcalidrawFonts(): Plugin {
  return {
    name: 'lattice-packaged-excalidraw-fonts',
    configureServer(server) {
      server.middlewares.use('/excalidraw-assets', (request, response, next) => {
        let relativePath: string
        try {
          relativePath = decodeURIComponent(request.url?.split('?')[0] ?? '').replace(/^\/+/, '')
        } catch {
          response.statusCode = 400
          response.end('Invalid font path')
          return
        }

        const filePath = path.resolve(excalidrawAssets, relativePath)
        const supportedAsset = relativePath === 'index.css'
          || (relativePath.startsWith(`fonts${path.sep}`) && relativePath.endsWith('.woff2'))
          || (relativePath.startsWith('fonts/') && relativePath.endsWith('.woff2'))
        if (!supportedAsset || !filePath.startsWith(`${excalidrawAssets}${path.sep}`)) {
          response.statusCode = 403
          response.end('Invalid font path')
          return
        }

        try {
          if (!statSync(filePath).isFile()) {
            next()
            return
          }
          response.setHeader('Content-Type', relativePath === 'index.css' ? 'text/css; charset=utf-8' : 'font/woff2')
          response.setHeader('Cache-Control', 'public, max-age=31536000, immutable')
          createReadStream(filePath).pipe(response)
        } catch {
          next()
        }
      })
    },
    writeBundle(outputOptions) {
      const outputDirectory = path.resolve(outputOptions.dir ?? 'dist')
      const destination = path.join(outputDirectory, 'excalidraw-assets')
      mkdirSync(destination, { recursive: true })
      cpSync(path.join(excalidrawAssets, 'index.css'), path.join(destination, 'index.css'))
      cpSync(path.join(excalidrawAssets, 'fonts'), path.join(destination, 'fonts'), {
        recursive: true,
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), packagedExcalidrawFonts()],
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
    manifest: true,
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
          // PDF.js is loaded only from the PDF viewer's lazy module. Keeping
          // it unassigned avoids creating a shared manual chunk that can pull
          // the viewer into unrelated initial-route dependencies.
          if (packageName === '@tiptap/pm' || packageName.startsWith('prosemirror-')) return 'tiptap-pm';
          if (packageName.startsWith('@tiptap/extension-') || packageName === 'tiptap-markdown') {
            return 'tiptap-extensions';
          }
          if (packageName === '@tiptap/react') return 'tiptap-react';
          if (packageName.startsWith('@tiptap/')) return 'tiptap-core';
          if (packageName === 'lowlight' || packageName === 'highlight.js') return 'syntax-languages';
          if (packageName === 'mammoth') return 'docx-renderer';
          if (packageName === 'dompurify') return 'sanitizer';
          // lucide-react, date-fns and cmdk stay unassigned: grouped, every icon
          // and helper any lazy page uses was loaded at startup. Unassigned,
          // each lands beside the pages that import it.
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
