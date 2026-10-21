import { expect, test } from '@playwright/test';

import { claimFixture, holdLessonForReview, unsupportedCommands } from './helpers/claimReviewFixtures';
import { installLearningStudioBackend } from './helpers/learningStudioBackend';

const ALMANAC = { sourceId: 'source-almanac', title: 'County Farmers’ Almanac, 1890', site: 'archive.example.org', kind: 'reference_work', url: 'https://archive.example.org/almanac-1890' };

const STATEMENTS = [
  'The last spring frost in the county fell on 12 May.',
  'Wheat was sown in the first week of April.',
  'The harvest fair was held in the second week of September.',
  'Rainfall in June exceeded four inches.',
  'The river froze over in early December.',
];

test('Accepting one source for all its claims verifies those it states, keeps the rest, and undoes in one step', async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: /Reasoning from field observations/ })).toBeVisible();
  const lesson = await holdLessonForReview(page, STATEMENTS.map((statement, index) => claimFixture({
    id: `almanac-${index + 1}`, statement, unitIndex: index % 2, source: ALMANAC, quote: statement,
    // The fifth: the almanac states it, and another source says otherwise.
    verdict: index === 4 ? 'contradicted' : 'insufficient_standing',
  })));
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  await page.locator(`#lesson-${lesson.id}`).getByRole('button', { name: 'Review', exact: true }).click();

  const queue = page.getByRole('navigation', { name: 'Claims waiting for you' });
  const group = queue.getByRole('region', { name: `${ALMANAC.title} · 5 claims` });
  await expect(group.getByText('archive.example.org · reference work · 5 claims')).toBeVisible();
  await group.getByRole('button', { name: 'Accept this source for all 5' }).click();

  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText(/5 claims rest on this source in this course/)).toBeVisible();
  await expect(dialog.getByRole('radio', { name: /a reliable identified source/ })).toBeChecked();
  await page.screenshot({ path: `e2e-results/claim-review-bulk-dialog-${browserName}.png` });
  await dialog.getByRole('button', { name: 'Accept for 5 claims' }).click();

  await expect(page.getByText(/Trusted County Farmers’ Almanac, 1890 for this course: 4 claims verified, 1 still needs you\./)).toBeVisible();
  await expect(page.getByTestId('review-tally')).toContainText('1 claim needs you');
  await expect(page.getByRole('article', { name: 'Claim in focus' }).getByText('The river froze over in early December.')).toBeVisible();

  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByText('Undone. 5 left.')).toBeVisible();
  await expect(page.getByTestId('review-tally')).toContainText('5 claims need you');

  expect(await unsupportedCommands(page)).toEqual([]);
});
