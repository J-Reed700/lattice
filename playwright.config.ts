import { defineConfig, devices } from '@playwright/test';

const WEB_PREVIEW_PORT = 4173;
const SMOKE_BUILD_DIR = 'e2e-results/renderer';

export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: [
    ['html', { outputFolder: 'e2e-results/html' }],
    ['json', { outputFile: 'e2e-results/results.json' }],
    ['list'],
  ],
  use: {
    baseURL: `http://127.0.0.1:${WEB_PREVIEW_PORT}`,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 10000,
  },
  timeout: 60000,
  expect: {
    timeout: 10000,
  },
  projects: [
    {
      name: 'web-smoke',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1280, height: 720 },
      },
    },
    {
      name: 'webkit-smoke',
      use: {
        ...devices['Desktop Safari'],
        viewport: { width: 1280, height: 720 },
      },
    },
  ],
  outputDir: 'e2e-results/artifacts',
  webServer: {
    // Browser smoke tests exercise the renderer. Full Tauri IPC integration
    // remains covered by Rust command/integration tests; tauri-driver is not
    // available on macOS.
    // Exercise the shipped renderer bundle, including lazy chunks and workers.
    // A separate, strict port prevents silently testing a developer's server.
    command: `npx vite build --outDir ${SMOKE_BUILD_DIR} && npx vite preview --outDir ${SMOKE_BUILD_DIR} --host 127.0.0.1 --port ${WEB_PREVIEW_PORT} --strictPort`,
    url: `http://127.0.0.1:${WEB_PREVIEW_PORT}`,
    timeout: 120000,
    reuseExistingServer: false,
  },
});
