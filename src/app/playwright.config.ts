import { defineConfig, devices } from '@playwright/test';
import path from 'path';

const TAURI_DEV_PORT = 1420;

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
    baseURL: `http://localhost:${TAURI_DEV_PORT}`,
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
      name: 'tauri-e2e',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1280, height: 720 },
      },
    },
  ],
  outputDir: 'e2e-results/artifacts',
  webServer: {
    command: 'npm run tauri:dev',
    url: `http://localhost:${TAURI_DEV_PORT}`,
    timeout: 120000,
    reuseExistingServer: !process.env.CI,
  },
});
