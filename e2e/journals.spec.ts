import { expect, type Page } from '@playwright/test';
import { gardenJournal, installJournalsFixture } from './fixtures/journals';
import { argsOf, journeyTest } from './fixtures/shell';

const test = journeyTest(installJournalsFixture);

/** The page body: the one rich-text box on the page (the title is a plain field). */
const body = (page: Page) => page.getByRole('textbox', { name: 'Journal entry', exact: true });
const title = (page: Page) => page.getByRole('textbox', { name: 'Page title' });

test('a first visit makes a journal and a page; writing autosaves and survives a reload and the Daily link', async ({ page, backend }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/journals\?journalSpaceId=journal-/);
  await expect(body(page)).toBeVisible();
  expect(argsOf(backend, 'create_journal').map(args => (args.request as { name: string }).name)).toEqual(['Journal 1']);
  expect(backend.state.pages).toHaveLength(1);

  await body(page).click();
  await page.keyboard.type('Sourdough starter fed at 7am; doubled by noon.');
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  await expect.poll(() => backend.state.pages[0].content).toContain('Sourdough starter fed at 7am; doubled by noon.');

  await title(page).fill('Bread notes');
  await title(page).press('Enter');
  await expect.poll(() => backend.state.pages[0].title).toBe('Bread notes');
  await expect(page.getByRole('complementary').getByText('Bread notes', { exact: true })).toBeVisible();

  await page.reload();
  await expect(body(page)).toContainText('Sourdough starter fed at 7am; doubled by noon.');
  await expect(title(page)).toHaveValue('Bread notes');

  // The old Daily route lands in the same journal, on the same page.
  await page.goto('/daily');
  await expect(page).toHaveURL(/\/journals\?journalSpaceId=journal-/);
  await expect(body(page)).toContainText('Sourdough starter fed at 7am; doubled by noon.');
  expect(argsOf(backend, 'create_journal')).toHaveLength(1);
});

test('a page save that fails says so and can be retried without losing the text', async ({ page, backend }) => {
  const garden = gardenJournal();
  backend.state.journals = [garden.journal];
  backend.state.pages = [garden.page];
  await page.goto('/journals');
  await expect(page).toHaveURL(/journalSpaceId=journal-garden/);
  await expect(title(page)).toHaveValue('Tomato trial');
  await expect(body(page)).toContainText('San Marzano seedlings went out on 2 May');

  backend.state.failNextSave = 'Disk is full.';
  await body(page).click();
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.type(' First trusses set on 30 June.');
  const retry = page.getByRole('button', { name: 'Save failed — retry?' });
  await expect(retry).toBeVisible();
  await expect(retry).toHaveAttribute('title', 'Disk is full.');
  expect(backend.state.pages[0].revision).toBe(3);

  await retry.click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  expect(backend.state.pages[0].revision).toBe(4);
  expect(backend.state.pages[0].content).toContain('First trusses set on 30 June.');
  await expect(body(page)).toContainText('First trusses set on 30 June.');
});

test('a journal list that cannot load explains itself and recovers on Try again', async ({ page, backend }) => {
  const garden = gardenJournal();
  backend.state.journals = [garden.journal];
  backend.state.pages = [garden.page];
  backend.state.listError = 'The journal database is locked by another window.';
  await page.goto('/journals');
  await expect(page.getByRole('heading', { name: 'Your journal couldn’t load' })).toBeVisible();
  await page.getByText('Error details').click();
  await expect(page.getByText('The journal database is locked by another window.')).toBeVisible();
  expect(argsOf(backend, 'create_journal')).toEqual([]);

  backend.state.listError = null;
  await page.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(title(page)).toHaveValue('Tomato trial');
  await expect(page).toHaveURL(/journalSpaceId=journal-garden/);
  expect(argsOf(backend, 'create_journal')).toEqual([]);
});
