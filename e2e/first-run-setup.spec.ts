import { expect, type Page } from '@playwright/test';

import type { RecommendationDto } from '../src/lib/bindings';
import { installHomeFixture } from './fixtures/home';
import { journeyTest } from './fixtures/shell';
import { mockCommands, serveCommands } from './fixtures/tauri';

const GB = 1_073_741_824;
const MODEL_COUNT = 14;

function recommendation(index: number): RecommendationDto {
  return {
    modelId: `model-${index}`,
    name: `Model ${index}`,
    description: 'A general model for everyday questions.',
    useFor: 'For chats and courses on this computer.',
    downloadBytes: 2.5 * GB,
    verdict: { kind: 'fits', needsBytes: 4 * GB, budgetBytes: 8 * GB, device: { name: 'Test GPU', kind: 'dedicated', totalBytes: 8 * GB } },
    labels: [],
    evidence: { kind: 'untested' },
    speed: 'balanced',
  };
}

/** A new computer: no models yet, and a long list the backend says would fit. */
async function installFirstRun(page: Page) {
  const backend = await installHomeFixture(page);
  await mockCommands(page, () => ({
    check_first_run_status: () => JSON.stringify({
      needs_setup: true,
      embedding_model: { model_id: 'bge-small', display_name: 'BGE Small', estimated_size_bytes: 134_217_728 },
      chat_model: { model_id: 'model-1', display_name: 'Model 1', estimated_size_bytes: 2_684_354_560 },
      total_estimated_size_bytes: 2_818_572_288,
    }),
  }), undefined);
  await serveCommands(page, () => ({
    recommend_chat_models: () => Array.from({ length: MODEL_COUNT }, (_, index) => recommendation(index + 1)),
    get_compatible_models: () => [],
    get_all_recommended_models: () => [],
  }));
  return backend;
}

const test = journeyTest(installFirstRun);

test('a long list of recommended models scrolls inside the setup dialog, which stays in the window', async ({ page }) => {
  await page.goto('/home');
  const dialog = page.getByRole('dialog', { name: 'Set up AI' });
  const models = dialog.getByRole('radio');
  await expect(models).toHaveCount(MODEL_COUNT);

  const viewport = page.viewportSize();
  const box = await dialog.boundingBox();
  expect(viewport).not.toBeNull();
  expect(box).not.toBeNull();
  if (!viewport || !box) return;
  expect(box.y, 'the dialog starts inside the window').toBeGreaterThanOrEqual(0);
  expect(box.y + box.height, 'the dialog ends inside the window').toBeLessThanOrEqual(viewport.height);

  const install = dialog.getByRole('button', { name: /^Install/ });
  await expect(dialog.getByRole('heading', { name: 'Set up AI' })).toBeInViewport({ ratio: 1 });
  await expect(install).toBeInViewport({ ratio: 1 });

  const last = models.last();
  await last.scrollIntoViewIfNeeded();
  await expect(last).toBeInViewport();
  await last.check();
  await expect(last).toBeChecked();
  await expect(install).toBeInViewport({ ratio: 1 });
});
