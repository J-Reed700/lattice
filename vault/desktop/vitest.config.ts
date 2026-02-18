import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'happy-dom',
    setupFiles: ['./websrc/tests/setup.ts'],
    include: ['websrc/**/*.{test,spec,integration.test}.{ts,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html', 'lcov'],
      include: ['websrc/**/*.{ts,tsx}'],
      exclude: [
        'websrc/**/*.test.{ts,tsx}',
        'websrc/**/*.integration.test.{ts,tsx}',
        'websrc/**/*.spec.{ts,tsx}',
        'websrc/tests/**',
        'websrc/main.tsx',
        'websrc/vite-env.d.ts',
        'websrc/lib/bindings.ts', // Exclude generated file
      ],
      all: true,
      lines: 70,
      functions: 70,
      branches: 70,
      statements: 70,
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './websrc'),
    },
  },
});
