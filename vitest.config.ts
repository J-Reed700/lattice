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
      all: true,
      // Vitest 4 only enforces floors nested under `thresholds`. Keep a
      // realistic whole-renderer baseline while applying a substantially
      // stronger contract to Learning Studio, where failures can corrupt a
      // learner's durable workflow. Raise these values as coverage grows.
      thresholds: {
        lines: 53,
        functions: 47,
        branches: 47,
        statements: 52,
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
