import { expect, type Page } from '@playwright/test';
import { installSearchFixture } from './fixtures/search';
import { argsOf, journeyTest } from './fixtures/shell';

type SearchBackend = Awaited<ReturnType<typeof installSearchFixture>>;
const test = journeyTest(installSearchFixture);

test('searching finds passages across documents, groups them per document, switches mode and opens results', async ({ page, backend }) => {
  await page.goto('/search');
  await expect(page.getByRole('heading', { name: 'Search', exact: true })).toBeVisible();
  await expect(page.getByText('Search across your sources', { exact: true })).toBeVisible();
  const input = page.getByRole('textbox', { name: 'Search your documents' });
  await expect(input).toBeFocused();

  await input.fill('photosynthesis');
  await expect(page.getByText('3 results', { exact: true })).toBeVisible();
  const results = page.getByRole('main').getByRole('button').filter({ hasText: /Plant biology notes|The light reactions|History of botany/ });
  await expect(results).toHaveCount(3);
  // Best first, and both passages of the plant notes under one result.
  await expect(results.first()).toContainText('Plant biology notes.md');
  await expect(results.first()).toContainText('Photosynthesis turns light, water and carbon dioxide into glucose and oxygen.');
  await expect(results.first()).toContainText('Stomata close in drought, which slows photosynthesis.');
  await expect(results.nth(1)).toContainText('The light reactions');
  await expect(results.nth(2)).toContainText('History of botany.pdf');
  expect(argsOf(backend, 'hybrid_search')).toEqual([{ args: { query: 'photosynthesis', limit: 20, search_mode: 'hybrid' } }]);

  await page.getByRole('tab', { name: 'Keyword', exact: true }).click();
  await expect(page.getByText('1 result', { exact: true })).toBeVisible();
  await expect(results).toHaveCount(1);
  expect(argsOf(backend, 'hybrid_search').at(-1)).toEqual({ args: { query: 'photosynthesis', limit: 20, search_mode: 'keyword' } });
  await page.getByRole('tab', { name: 'Hybrid', exact: true }).click();
  await expect(results).toHaveCount(3);

  // A saved web page opens in the in-app reader at the matching passage.
  await results.filter({ hasText: 'The light reactions' }).click();
  const reader = page.getByRole('dialog');
  await expect(reader).toContainText('chlorophyll absorbs red and blue light');
  await page.keyboard.press('Escape');
  await expect(reader).toBeHidden();

  // A local note is handed to the system viewer.
  await results.filter({ hasText: 'Plant biology notes.md' }).click();
  await expect.poll(() => argsOf(backend, 'open_file_by_id')).toEqual([{ fileId: 'doc-plants' }]);
  await expect(reader).toHaveCount(0);
});

test('a search with no matches says so, and a recent search can be picked again', async ({ page, backend }) => {
  await page.addInitScript(() => {
    localStorage.setItem('lattice-command-palette', JSON.stringify({ recentSearches: [{ id: 'recent-1', label: 'sourdough hydration' }] }));
  });
  await page.goto('/search');
  await expect(page.getByRole('heading', { name: 'Recent' })).toBeVisible();
  await page.getByRole('button', { name: 'sourdough hydration', exact: true }).click();

  await expect(page.getByRole('textbox', { name: 'Search your documents' })).toHaveValue('sourdough hydration');
  await expect(page.getByText('No results for “sourdough hydration”.')).toBeVisible();
  await expect(page.getByText('Try a broader term or a different search mode.')).toBeVisible();
  expect(argsOf(backend, 'hybrid_search')).toEqual([{ args: { query: 'sourdough hydration', limit: 20, search_mode: 'hybrid' } }]);
  expect(argsOf(backend, 'search_documents')).toEqual([{ options: { query: 'sourdough hydration', limit: 20, searchMode: 'hybrid' } }]);

  // Clearing the box returns to the start screen.
  await page.getByRole('textbox', { name: 'Search your documents' }).fill('');
  await expect(page.getByRole('button', { name: 'sourdough hydration', exact: true })).toBeVisible();
});

test('a result whose file is no longer on disk says so instead of opening nothing', async ({ page, backend }) => {
  await page.goto('/search');
  await page.getByRole('textbox', { name: 'Search your documents' }).fill('photosynthesis');
  await page.getByRole('main').getByRole('button').filter({ hasText: 'History of botany.pdf' }).click();
  await expect(page.getByText('No file at /home/test/Archive/History of botany.pdf.', { exact: true })).toBeVisible();
  expect(argsOf(backend, 'open_file_by_id')).toEqual([{ fileId: 'doc-history' }]);
  expect(argsOf(backend, 'open_file')).toEqual([{ path: '/home/test/Archive/History of botany.pdf' }]);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

/** Types a query whose search fails on every attempt, and lets the retries run out. */
async function failEverySearch(page: Page, backend: SearchBackend, query: string) {
  backend.state.searchError = 'The search index is being rebuilt.';
  const before = argsOf(backend, 'hybrid_search').length;
  await page.getByRole('textbox', { name: 'Search your documents' }).fill(query);
  // A failed search is retried with backoff before the reader is told.
  for (const attempt of [1, 2, 3]) {
    await expect.poll(() => argsOf(backend, 'hybrid_search').length).toBe(before + attempt);
    await page.clock.fastForward(30_000);
  }
  await expect(page.getByText("Couldn't search. The search index is being rebuilt.", { exact: true })).toBeVisible();
  expect(argsOf(backend, 'hybrid_search')).toHaveLength(before + 4);
}

test('a search that keeps failing says why, and the next query recovers', async ({ page, backend }) => {
  await page.clock.install();
  await page.goto('/search');
  await failEverySearch(page, backend, 'photosynthesis in algae');

  backend.state.searchError = null;
  await page.getByRole('textbox', { name: 'Search your documents' }).fill('photosynthesis in leaves');
  await expect(page.getByText('3 results', { exact: true })).toBeVisible();
  await expect(page.getByText(/Couldn.t search/)).toHaveCount(0);
});

test('a failed search is not reported as a search with no matches, and Try again searches again', async ({ page, backend }) => {
  await page.clock.install();
  await page.goto('/search');
  await failEverySearch(page, backend, 'photosynthesis in algae');
  await expect(page.getByText(/No results for/)).toHaveCount(0);
  await expect(page.getByText('Try a broader term or a different search mode.')).toHaveCount(0);

  backend.state.searchError = null;
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(page.getByText('3 results', { exact: true })).toBeVisible();
  await expect(page.getByText(/Couldn.t search/)).toHaveCount(0);
  expect(argsOf(backend, 'hybrid_search').at(-1)).toMatchObject({ args: { query: 'photosynthesis in algae' } });
});
