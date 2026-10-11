import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    // DOMPurify explicitly requires a standards-compliant DOM for security;
    // happy-dom is not a supported sanitizer runtime.
    environment: 'jsdom',
    setupFiles: ['./src/tests/setup.ts'],
    include: ['src/**/*.{test,spec,integration.test}.{ts,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'json-summary', 'html', 'lcov'],
      reportOnFailure: true,
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/**/*.test.{ts,tsx}',
        'src/**/*.integration.test.{ts,tsx}',
        'src/**/*.spec.{ts,tsx}',
        'src/tests/**',
        'src/main.tsx',
        'src/vite-env.d.ts',
        'src/lib/bindings.ts', // Exclude generated file
      ],
      // Vitest 4 only enforces floors nested under `thresholds`. Keep a
      // realistic whole-renderer baseline while applying a substantially
      // stronger contract to Learning Studio, where failures can corrupt a
      // learner's durable workflow. Raise these values as coverage grows.
      thresholds: {
        lines: 61,
        functions: 55,
        branches: 55.5,
        statements: 60,
        // These lifecycle, I/O and import boundaries must retain complete
        // coverage individually; a well-covered sibling cannot hide a gap.
        'src/{lib/pendingSaves,features/reading/hooks/useFileContent,hooks/useDebounce,utils/batchHistory,utils/batchImport,features/model/components/Downloads/downloadFormat}.ts': {
          perFile: true,
          lines: 100,
          functions: 100,
          branches: 100,
          statements: 100,
        },
        'src/features/files/components/Ingest/BatchUrlImport.tsx': {
          lines: 92,
          functions: 95,
          branches: 75,
          statements: 89,
        },
        'src/utils/{dateUtils,fileTypeDetector,toast}.ts': {
          perFile: true,
          lines: 100,
          functions: 100,
          branches: 100,
          statements: 100,
        },
        'src/utils/{fileSources,promiseHandlers,sourcePreview}.ts': {
          perFile: true,
          lines: 96,
          functions: 96,
          branches: 80,
          statements: 96,
        },
        'src/features/learning/{workspace,curriculum,lessons,memory,sources,practice,assessment,recall,canvas,portability}/**': {
          lines: 87,
          functions: 75,
          branches: 67,
          statements: 77,
        },
      },
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, './src'),
    },
  },
});
