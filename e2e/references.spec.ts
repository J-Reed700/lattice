import { expect, type Page } from '@playwright/test';
import { installReferencesFixture } from './fixtures/references';
import { gate, journeyTest } from './fixtures/shell';

const test = journeyTest(installReferencesFixture);

const sidebar = (page: Page) => page.getByRole('complementary').filter({ has: page.getByRole('heading', { name: 'References', exact: true }) });
const row = (page: Page, text: string) => sidebar(page).getByRole('button').filter({ hasText: text });

test('saved passages and chat answers share one inbox: read, annotate, search, open the source and delete', async ({ page, backend }) => {
  await page.goto('/references');
  await expect(sidebar(page).getByText('3 references', { exact: true })).toBeVisible();
  await expect(sidebar(page).getByRole('heading', { name: 'Today' })).toBeVisible();
  await expect(sidebar(page).getByRole('heading', { name: 'Yesterday' })).toBeVisible();

  await row(page, 'Cell biology primer.md').click();
  await expect(row(page, 'Cell biology primer.md')).toHaveAttribute('aria-current', 'page');
  await expect(page).toHaveURL(/referenceId=ref-mitochondria/);
  const reader = page.getByRole('main');
  await expect(reader.getByText('Mitochondria generate most of the cell’s ATP through oxidative phosphorylation.')).toBeVisible();

  await reader.getByRole('button', { name: 'Add context or a takeaway…' }).click();
  await reader.getByRole('textbox', { name: 'Reference note' }).fill('Most ATP comes from here, not from glycolysis.');
  await reader.getByRole('textbox', { name: 'Reference note' }).press('Control+Enter');
  await expect(reader.getByRole('button', { name: 'Most ATP comes from here, not from glycolysis.' })).toBeVisible();
  await reader.getByRole('button', { name: 'Add a title…' }).click();
  await reader.getByRole('textbox', { name: 'Reference title' }).fill('Powerhouse');
  await reader.getByRole('textbox', { name: 'Reference title' }).press('Enter');
  await expect(row(page, 'Powerhouse')).toBeVisible();
  expect(backend.state.passages.find(item => item.id === 'ref-mitochondria')).toMatchObject({ title: 'Powerhouse', note: 'Most ATP comes from here, not from glycolysis.' });

  // A reload returns to the same reference with its annotations.
  await page.reload();
  await expect(row(page, 'Powerhouse')).toHaveAttribute('aria-current', 'page');
  await expect(reader.getByRole('button', { name: 'Most ATP comes from here, not from glycolysis.' })).toBeVisible();

  // A bookmarked chat answer shows the whole answer, not just its preview.
  await row(page, 'Bread baking').click();
  await expect(reader.getByText(/so the gluten starts forming on its own/)).toBeVisible();
  await expect(page).toHaveURL(/referenceId=bookmark-autolyse/);

  const search = sidebar(page).getByRole('searchbox', { name: 'Search references' });
  await search.fill('magna');
  await expect(sidebar(page).getByText('1 reference', { exact: true })).toBeVisible();
  await expect(row(page, 'Rule of law')).toBeVisible();
  await search.fill('zeppelin');
  await expect(sidebar(page).getByText('No references match.')).toBeVisible();
  await search.fill('');
  await expect(sidebar(page).getByText('3 references', { exact: true })).toBeVisible();

  await row(page, 'Rule of law').click();
  await expect(reader.getByText('Compare with the 1689 Bill of Rights.')).toBeVisible();
  await reader.getByRole('button', { name: 'Open source', exact: true }).click();
  const source = page.getByRole('dialog');
  await expect(source).toContainText('Magna Carta (1215) bound the king to the law of the land.');
  await page.keyboard.press('Escape');
  await expect(source).toBeHidden();

  await reader.getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.getByText('Reference removed')).toBeVisible();
  await expect(row(page, 'Rule of law')).toHaveCount(0);
  await expect(sidebar(page).getByText('2 references', { exact: true })).toBeVisible();
  expect(backend.state.passages.map(item => item.id)).toEqual(['ref-mitochondria']);
});

test('an empty inbox says nothing has been saved yet', async ({ page, backend }) => {
  backend.state.passages = [];
  backend.state.bookmarks = [];
  await page.goto('/references');
  await expect(sidebar(page).getByText('No references yet.')).toBeVisible();
  await expect(sidebar(page).getByText('0 references', { exact: true })).toBeVisible();
  await expect(sidebar(page).getByText(/Couldn.t load references/)).toHaveCount(0);
});

test('an inbox that cannot load offers a retry, and a failed note save can be retried', async ({ page, backend }) => {
  backend.state.bookmarks = [];
  backend.state.listError = 'The reference store is locked by another window.';
  await page.goto('/references');
  await expect(sidebar(page).getByText(/Couldn.t load references\./)).toBeVisible();
  await expect(sidebar(page).getByText('No references yet.')).toHaveCount(0);

  backend.state.listError = null;
  await sidebar(page).getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(row(page, 'Cell biology primer.md')).toBeVisible();

  const reader = page.getByRole('main');
  backend.state.failNextUpdate = 'Disk is full.';
  await reader.getByRole('button', { name: 'Add context or a takeaway…' }).click();
  await reader.getByRole('textbox', { name: 'Reference note' }).fill('Ask how brown fat uncouples this.');
  await reader.getByRole('textbox', { name: 'Reference note' }).press('Control+Enter');
  await reader.getByRole('button', { name: 'Save failed — retry?' }).click();
  await expect(reader.getByRole('textbox', { name: 'Reference note' })).toHaveValue('Ask how brown fat uncouples this.');
  await reader.getByRole('textbox', { name: 'Reference note' }).press('Control+Enter');
  await expect(reader.getByRole('button', { name: 'Ask how brown fat uncouples this.' })).toBeVisible();
  await expect(reader.getByRole('button', { name: 'Save failed — retry?' })).toHaveCount(0);
  expect(backend.state.passages[0].note).toBe('Ask how brown fat uncouples this.');
});

test('saved passages that fail to load are not silently dropped when bookmarks load', async ({ page, backend }) => {
  backend.state.listError = 'The reference store is locked by another window.';
  await page.goto('/references');
  await expect(row(page, 'Bread baking')).toBeVisible();
  await expect(sidebar(page).getByText(/Couldn.t load all references\./)).toBeVisible();
  await expect(row(page, 'Cell biology primer.md')).toHaveCount(0);

  backend.state.listError = null;
  await sidebar(page).getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(row(page, 'Cell biology primer.md')).toBeVisible();
  await expect(row(page, 'Bread baking')).toBeVisible();
  await expect(sidebar(page).getByText(/Couldn.t load/)).toHaveCount(0);
  await expect(sidebar(page).getByText('3 references', { exact: true })).toBeVisible();
});

test('a link to a saved passage opens that passage when bookmarks load first', async ({ page, backend }) => {
  const passages = gate();
  backend.state.holdPassages = passages.released;
  await page.goto('/references?referenceId=ref-mitochondria');
  // The bookmark has reached the inbox; the saved passages have not.
  await expect(sidebar(page).getByText('1 reference', { exact: true })).toBeVisible();
  await expect(page).toHaveURL(/\/references\?referenceId=ref-mitochondria$/);

  passages.release();
  await expect(row(page, 'Cell biology primer.md')).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('main').getByText('Mitochondria generate most of the cell’s ATP through oxidative phosphorylation.')).toBeVisible();
  await expect(page).toHaveURL(/\/references\?referenceId=ref-mitochondria$/);
});
