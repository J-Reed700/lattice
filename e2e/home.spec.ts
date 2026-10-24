import { expect, type Page } from '@playwright/test';
import { installHomeFixture } from './fixtures/home';
import { argsOf, gate, journeyTest } from './fixtures/shell';

const test = journeyTest(installHomeFixture);

const section = (page: Page, name: string) => page.getByRole('main').locator('section').filter({ has: page.getByRole('heading', { name, exact: true }) });
const tile = (page: Page, label: string) => section(page, 'Library').getByText(label, { exact: true }).locator('xpath=..');
const GREETING = /^(Still up|Good morning|Good afternoon|Good evening)\.$/;

test('Home reads out the library, lists what to resume and leads on to all references', async ({ page }) => {
  await page.goto('/home');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(GREETING);

  await expect(tile(page, 'Documents')).toContainText('3');
  await expect(tile(page, 'Folders')).toContainText('2');
  await expect(tile(page, 'On disk')).toBeVisible();
  await expect(tile(page, 'Last indexed')).toBeVisible();

  // Journal entries are not chats to resume; today's entry has its own row.
  const resume = section(page, 'Continue').getByRole('button').filter({ hasNotText: 'All conversations' });
  await expect(resume).toHaveCount(2);
  await expect(resume.first()).toContainText('Bread baking');
  await expect(resume.nth(1)).toContainText('Roman roads');
  await expect(section(page, 'Today')).toContainText('Morning birdsong count');
  await expect(section(page, 'Today')).toContainText('4 messages');
  await expect(section(page, 'This week')).toBeVisible();

  const recent = section(page, 'Recently indexed').getByRole('button');
  await expect(recent).toHaveText([/Cell biology primer\.md/, /Medieval England\.md/, /Field recordings of songbirds\.pdf/]);

  await expect(section(page, 'Saved').getByRole('button').filter({ hasText: 'Bread baking' })).toContainText('Chat · Bread baking');

  await section(page, 'Saved').getByRole('button', { name: 'All references', exact: true }).click();
  await expect(page).toHaveURL(/\/references/);
  await expect(page.getByText('3 references', { exact: true })).toBeVisible();
});

test('a saved answer on Home opens that answer in References', async ({ page, backend }) => {
  // Saved passages are made to reach the inbox before the bookmarks, so the
  // inbox must not open a passage in place of the linked answer meanwhile.
  const bookmarks = gate();
  await page.goto('/home');
  const saved = section(page, 'Saved').getByRole('button').filter({ hasText: 'Bread baking' });
  await expect(saved).toBeVisible();
  backend.state.holdBookmarks = bookmarks.released;
  await saved.click();
  // Saved passages reach the inbox before the bookmarks do.
  await expect(page.getByText('2 references', { exact: true })).toBeVisible();
  await expect(page).toHaveURL(/\/references\?referenceId=bookmark-autolyse$/);
  bookmarks.release();
  await expect(page.getByRole('main').getByText(/so the gluten starts forming on its own/)).toBeVisible();
  await expect(page).toHaveURL(/\/references\?referenceId=bookmark-autolyse$/);
});

test('a new library shows where things will appear instead of zeros and blanks', async ({ page, backend }) => {
  Object.assign(backend.state, { documents: [], folders: [], conversations: [], journals: [], passages: [], bookmarks: [] });
  await page.goto('/home');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(GREETING);
  await expect(page.getByText('Your conversations will appear here. Start one above to explore an idea.')).toBeVisible();
  await expect(page.getByText('Nothing saved yet. Bookmark a message to keep it here.')).toBeVisible();
  await expect(page.getByText('Add your first files to make your library searchable.')).toBeVisible();
  await expect(tile(page, 'Documents')).toContainText('0');
  await expect(tile(page, 'Last indexed')).toContainText('Never');
  await expect(section(page, 'Today').getByRole('button', { name: 'Start today’s entry', exact: true })).toBeVisible();
  await expect(section(page, 'This week')).toHaveCount(0);
});

test('Home that cannot load says so and loads again on Try again', async ({ page, backend }) => {
  await page.clock.install();
  backend.state.statsError = 'The library database is locked.';
  await page.goto('/home');
  // Home's query is retried with backoff before the failure is shown.
  for (const attempt of [1, 2, 3]) {
    await expect.poll(() => argsOf(backend, 'get_indexing_stats').length).toBe(attempt);
    await page.clock.fastForward(30_000);
  }
  await expect(page.getByText("Couldn't load Home.", { exact: true })).toBeVisible();
  await expect(page.getByText('The library database is locked.', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { level: 1 })).toHaveCount(0);

  backend.state.statsError = null;
  await page.getByRole('button', { name: 'Try again' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(GREETING);
  await expect(tile(page, 'Documents')).toContainText('3');
});
