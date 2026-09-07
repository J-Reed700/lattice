import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('lattice:first-run-skipped', 'true');

    let nextCallbackId = 1;
    let nextListenerId = 1;
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();

    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { label: 'main' },
        },
        transformCallback(callback: (...args: unknown[]) => unknown, once = false) {
          const id = nextCallbackId++;
          callbacks.set(id, once ? (...args) => {
            callbacks.delete(id);
            return callback(...args);
          } : callback);
          return id;
        },
        unregisterCallback(id: number) {
          callbacks.delete(id);
        },
        async invoke(command: string) {
          if (command === 'plugin:event|listen') {
            return nextListenerId++;
          }
          if (command === 'plugin:event|unlisten') {
            return undefined;
          }
          if (command === 'plugin:model|check_first_run_status') {
            return JSON.stringify({
              needs_setup: false,
              recommended_model_id: null,
              recommended_model_name: null,
              estimated_size_bytes: null,
            });
          }
          throw new Error(`Tauri backend unavailable in renderer smoke test: ${command}`);
        },
      },
    });

    Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
      configurable: true,
      value: { unregisterListener: () => undefined },
    });
  });
});

test('renders the application shell and navigates to settings', async ({ page }) => {
  const pageErrors: Error[] = [];
  page.on('pageerror', (error) => pageErrors.push(error));

  await page.goto('/');

  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Journal', exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  expect(pageErrors).toEqual([]);
});
