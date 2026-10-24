import { expect } from '@playwright/test';
import { COLUMNS, installCompareFixture } from './fixtures/compare';
import { argsOf, gate, journeyTest } from './fixtures/shell';

const test = journeyTest(installCompareFixture);

test('compares recipes picked in the Library, opens a cited sentence and saves the table to the journal', async ({ page, backend }) => {
  await page.goto('/files');
  await page.getByRole('button', { name: 'List view', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Select Weeknight shakshuka.md', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Select Red lentil dal.md', exact: true }).click();
  await page.getByLabel('Document selection actions').getByRole('button', { name: 'Compare', exact: true }).click();

  await expect(page).toHaveURL(/\/compare\?ids=doc-shakshuka,doc-dal$/);
  await expect(page.getByRole('heading', { name: 'Compare', exact: true })).toBeVisible();
  await expect(page.getByText('2 documents', { exact: true })).toBeVisible();

  // Nothing runs until the reader names a column.
  const columns = page.getByLabel('Columns', { exact: true });
  const run = page.getByRole('button', { name: 'Compare', exact: true });
  await expect(run).toBeDisabled();
  await columns.press('Enter');
  await expect(page.getByText('Name at least one column.')).toBeVisible();
  await page.getByRole('button', { name: COLUMNS.join(', '), exact: true }).click();
  await expect(columns).toHaveValue(COLUMNS.join(', '));
  await expect(page.getByText('3 columns', { exact: true })).toBeVisible();

  const reply = gate();
  backend.state.hold = reply.released;
  await run.click();
  await expect(page.getByText('Reading 2 documents…')).toBeVisible();
  await expect(run).toBeDisabled();
  reply.release();

  const table = page.getByRole('table');
  await expect(table.getByRole('columnheader')).toHaveText(['Document', ...COLUMNS]);
  const dal = table.getByRole('row').filter({ hasText: 'Red lentil dal.md' });
  await expect(dal.getByRole('cell')).toHaveText([/Red lentil dal\.md/, /^40 minutes/, 'not stated', /^Red lentils/]);
  await expect(table.getByRole('row').filter({ hasText: 'Weeknight shakshuka.md' }).getByRole('cell').nth(2)).toContainText('4');
  await expect(page.getByText('Filled by Qwen3.5-9B.', { exact: false })).toBeVisible();
  await expect(page).toHaveURL(/columns=prep\+time%2C\+servings%2C\+main\+protein/);
  expect(argsOf(backend, 'compare_documents')).toEqual([{ request: { documentIds: ['doc-shakshuka', 'doc-dal'], columns: COLUMNS } }]);

  // The quote under a value opens the document at that sentence.
  await dal.getByRole('button', { name: /Simmer for 40 minutes/ }).click();
  const reader = page.getByRole('dialog');
  await expect(reader).toContainText('Simmer for 40 minutes until the lentils collapse.');
  await page.keyboard.press('Escape');
  await expect(reader).toBeHidden();

  await page.getByRole('button', { name: 'Save to journal', exact: true }).click();
  await expect(page.getByText('Comparison saved to "Kitchen notebook".')).toBeVisible();
  expect(backend.state.captured).toHaveLength(1);
  expect(backend.state.captured[0]).toContain('| Document | prep time | servings | main protein |');
  expect(backend.state.captured[0]).toContain('| Red lentil dal.md | 40 minutes | not stated | Red lentils |');
});

test('Compare explains what it needs when fewer than two documents are chosen', async ({ page, backend }) => {
  await page.goto('/compare?ids=doc-dal');
  await expect(page.getByText('Nothing to compare yet', { exact: true })).toBeVisible();
  await expect(page.getByText('Select two or more documents in the Library to compare them.')).toBeVisible();
  await expect(page.getByLabel('Columns', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: /Open Library/ }).click();
  await expect(page).toHaveURL(/\/files$/);
  expect(argsOf(backend, 'compare_documents')).toEqual([]);
});

test('a failed comparison can be retried, and a document the model could not finish says why', async ({ page, backend }) => {
  backend.state.failNext = 'The chat model is not loaded.';
  const query = new URLSearchParams({ ids: 'doc-shakshuka,doc-coq', columns: COLUMNS.join(', ') });
  await page.goto(`/compare?${query.toString()}`);

  await expect(page.getByText('Couldn’t build the table. The chat model is not loaded.')).toBeVisible();
  await expect(page.getByRole('table')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Save to journal' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Try again', exact: true }).click();

  const coq = page.getByRole('table').getByRole('row').filter({ hasText: 'Coq au vin.md' });
  await expect(coq).toContainText('Stopped after the first column: the model took too long on this document.');
  await expect(coq.getByRole('cell')).toHaveText([/Coq au vin\.md/, /^Overnight plus two hours/, 'not stated', 'not stated']);
  expect(argsOf(backend, 'compare_documents')).toHaveLength(2);

  // The selection and columns live in the URL, so a reload asks again for the same table.
  await page.reload();
  await expect(coq.getByRole('cell').nth(1)).toContainText('Overnight plus two hours');
  expect(argsOf(backend, 'compare_documents')).toHaveLength(3);
});
