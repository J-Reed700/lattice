import { expect, test as base } from '@playwright/test';
import { installExplorerFixture, OTHER_ROOT, ROOT } from './fixtures/explorer';

const test = base.extend<{ backend: Awaited<ReturnType<typeof installExplorerFixture>> }>({
  backend: [async ({ page }, use) => {
    const backend = await installExplorerFixture(page);
    await use(backend);
    expect(backend.unsupported, 'Every IPC dependency must have an explicit fixture contract').toEqual([]);
    expect(backend.pageErrors, 'No uncaught renderer exceptions').toEqual([]);
  }, { auto: true }],
});

async function openFolder(page: import('@playwright/test').Page) {
  await page.goto('/explorer');
  await page.getByRole('button', { name: /Choose a folder/ }).click();
  await expect(page.getByRole('tree')).toBeVisible();
}

for (const width of [1440, 800]) {
  test(`Explorer keyboard browsing, search, history and restart at ${width}px`, async ({ page, backend }) => {
    await page.setViewportSize({ width, height: 900 });
    await openFolder(page);
    const tree = page.getByRole('tree');
    await tree.focus();
    await tree.press('ArrowDown');
    await tree.press('ArrowRight');
    await expect(page.getByRole('treeitem', { name: 'hello.ts', exact: true })).toBeVisible();
    await page.getByRole('treeitem', { name: 'hello.ts', exact: true }).click();
    await expect(page.getByLabel('Contents of src/hello.ts')).toContainText('answer = 42');
    await page.getByRole('treeitem', { name: 'README.md', exact: true }).click();
    await expect(page.getByLabel('Contents of README.md')).toContainText(ROOT);
    await page.getByRole('button', { name: 'Back', exact: true }).click();
    await expect(page.getByLabel('Contents of src/hello.ts')).toBeVisible();
    await page.getByRole('button', { name: 'Forward', exact: true }).click();
    await expect(page.getByLabel('Contents of README.md')).toBeVisible();

    const search = page.getByRole('searchbox', { name: 'Search in folder' });
    await search.fill('answer');
    await page.getByRole('button', { name: /src\/hello.ts:2/ }).click();
    await expect(page.locator('.cm-explorer-highlight')).toContainText('answer = 42');
    expect(backend.calls.filter(call => call.command.endsWith('|explorer_search')).at(-1)?.args).toMatchObject({ root: ROOT, query: 'answer', maxResults: 200 });
    await search.fill('no-such-symbol');
    await expect(page.getByText('No matches in 6 files.')).toBeVisible();
    await search.press('Escape');
    await expect(tree).toBeVisible();

    await page.getByRole('button', { name: 'Hide folder tree' }).click();
    await page.reload();
    await expect(page.getByRole('button', { name: 'Show folder tree' })).toBeVisible();
    await page.getByRole('button', { name: 'Show folder tree' }).click();
    await expect(tree).toBeVisible();
    await page.getByRole('button', { name: 'Close folder', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Pick the folder to work in.' })).toBeVisible();
    expect(backend.state.closeCount).toBe(1);
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Pick the folder to work in.' })).toBeVisible();
  });
}

test('Explorer handles empty, binary, oversized and deleted files without breaking navigation', async ({ page }) => {
  await openFolder(page);
  await page.getByRole('treeitem', { name: 'empty.txt', exact: true }).click();
  await expect(page.getByLabel('Contents of empty.txt')).toHaveText('');
  await page.getByRole('treeitem', { name: 'image.bin', exact: true }).click();
  await expect(page.getByText('This is a binary file.')).toBeVisible();
  await page.getByRole('treeitem', { name: 'large.log', exact: true }).click();
  await expect(page.getByText('This file is too large to show.')).toBeVisible();
  await page.getByRole('treeitem', { name: 'missing.txt', exact: true }).click();
  await expect(page.getByRole('alert').filter({ hasText: 'No file in this folder matches that path.' })).toBeVisible();
  await page.getByRole('treeitem', { name: 'README.md', exact: true }).click();
  await expect(page.getByLabel('Contents of README.md')).toContainText('Read this project first.');
});

test('Explorer picker cancellation and permission failure preserve the start screen and allow retry', async ({ page, backend }) => {
  backend.state.picker = null;
  await page.goto('/explorer');
  await page.getByRole('button', { name: /Choose a folder/ }).click();
  await expect(page.getByRole('heading', { name: 'Pick the folder to work in.' })).toBeVisible();
  expect(backend.calls.some(call => call.command.endsWith('|explorer_resolve_root'))).toBe(false);
  backend.state.picker = ROOT;
  backend.state.failOpen = true;
  await page.getByRole('button', { name: /Choose a folder/ }).click();
  await expect(page.getByText('Folder permission denied', { exact: true })).toBeVisible();
  await expect(page.getByRole('tree')).toHaveCount(0);
  backend.state.failOpen = false;
  await page.getByRole('button', { name: /Choose a folder/ }).click();
  await expect(page.getByRole('tree')).toBeVisible();
});

test('switching Explorer roots keeps cached file contents and search requests scoped to the new folder', async ({ page, backend }) => {
  await openFolder(page);
  await page.getByRole('treeitem', { name: 'README.md', exact: true }).click();
  await expect(page.getByLabel('Contents of README.md')).toContainText(ROOT);
  await page.getByRole('button', { name: 'Close folder', exact: true }).click();
  backend.state.picker = OTHER_ROOT;
  await page.getByRole('button', { name: /Choose a folder/ }).click();
  await page.getByRole('treeitem', { name: 'README.md', exact: true }).click();
  await expect(page.getByLabel('Contents of README.md')).toContainText(OTHER_ROOT);
  await expect(page.getByLabel('Contents of README.md')).not.toContainText(ROOT);
  await expect(page.getByRole('button', { name: 'Back', exact: true })).toBeDisabled();
  await page.getByRole('searchbox', { name: 'Search in folder' }).fill('answer');
  await expect(page.getByRole('button', { name: /src\/hello.ts:2/ })).toBeVisible();
  expect(backend.calls.filter(call => call.command.endsWith('|explorer_search')).at(-1)?.args.root).toBe(OTHER_ROOT);
});

test('Explorer recovers from a search failure without losing the open file', async ({ page, backend }) => {
  await openFolder(page);
  await page.getByRole('treeitem', { name: 'README.md', exact: true }).click();
  backend.state.failSearch = true;
  const search = page.getByRole('searchbox', { name: 'Search in folder' });
  await search.fill('broken');
  await expect(page.getByRole('alert').filter({ hasText: 'Search index unavailable' })).toBeVisible();
  await expect(page.getByLabel('Contents of README.md')).toContainText(ROOT);
  backend.state.failSearch = false;
  await search.fill('answer');
  await expect(page.getByRole('button', { name: /src\/hello.ts:2/ })).toBeVisible();
});
