import { defineConfig, devices } from '@playwright/test';
import path from 'path';

const WEB_DEV_PORT = 5173;

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
    baseURL: `http://127.0.0.1:${WEB_DEV_PORT}`,
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
  ],
  outputDir: 'e2e-results/artifacts',
  webServer: {
    // Browser smoke tests exercise the renderer. Full Tauri IPC integration
    // remains covered by Rust command/integration tests; tauri-driver is not
    // available on macOS.
    command: 'npx vite --host 127.0.0.1',
    url: `http://127.0.0.1:${WEB_DEV_PORT}`,
    timeout: 120000,
    reuseExistingServer: !process.env.CI,
  },
});
