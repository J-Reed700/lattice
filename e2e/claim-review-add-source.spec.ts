import { expect, test } from '@playwright/test';

import { claimFixture, holdLessonForReview, unsupportedCommands } from './helpers/claimReviewFixtures';
import { installLearningStudioBackend } from './helpers/learningStudioBackend';
import { openStudioSection } from './helpers/learningStudioNavigation';

const STATEMENTS = [
  'Monarch butterflies migrate up to 3,000 miles.',
  'The eastern population winters in central Mexico.',
  'Milkweed is the only food of monarch caterpillars.',
  'The return trip takes several generations.',
  'Monarchs navigate using a time-compensated sun compass.',
];

test('One page added for the whole queue checks every claim, and a source added on the Sources tab checks what still waits', async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: /Reasoning from field observations/ })).toBeVisible();
  const lesson = await holdLessonForReview(page, STATEMENTS.map((statement, index) => claimFixture({ id: `monarch-${index + 1}`, statement, unitIndex: index % 3, verdict: 'unsupported' })));
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  await page.locator(`#lesson-${lesson.id}`).getByRole('button', { name: 'Review', exact: true }).click();

  const queue = page.getByRole('navigation', { name: 'Claims waiting for you' });
  await queue.getByRole('button', { name: 'Add a source for these 5' }).click();
  const sheet = page.getByRole('region', { name: 'Add a source for all 5 claims' });
  await sheet.getByRole('button', { name: 'Paste text instead' }).click();
  await sheet.getByLabel('Where it is from').fill('Monarch field guide, chapter 3');
  // States the first three; says nothing of the last two.
  await sheet.getByLabel('Text').fill(`${STATEMENTS[0]}\n${STATEMENTS[1]}\n${STATEMENTS[2]}`);
  await page.screenshot({ path: `e2e-results/claim-review-add-source-sheet-${browserName}.png` });
  await sheet.getByRole('button', { name: 'Add and check' }).click();

  await expect(page.getByText(/Verified 3 of 5 claims/)).toBeVisible();
  await expect(queue.getByRole('button', { name: 'Show resolved (3)' })).toBeVisible();
  await expect(page.getByTestId('review-tally')).toContainText('2 claims need you');

  // The Sources tab: a text that states a fourth claim checks the two still waiting.
  await page.getByRole('button', { name: lesson.title, exact: true }).click();
  await expect(page).not.toHaveURL(/review=claims/);
  await openStudioSection(page, 'Sources');
  await page.getByRole('button', { name: 'Add material', exact: true }).click();
  const addDialog = page.getByRole('dialog', { name: 'Bring in a source' });
  await addDialog.getByRole('button', { name: 'Paste text', exact: true }).click();
  await addDialog.getByLabel('Title', { exact: true }).fill('Monarch life cycle notes');
  await addDialog.getByLabel('Saved text').fill(STATEMENTS[3]);
  // On by default: the new source is checked against the claims still waiting.
  await expect(addDialog.getByRole('checkbox', { name: 'Check the waiting claims against it' })).toBeChecked();
  await addDialog.getByRole('button', { name: 'Save source' }).click();
  await expect(addDialog).not.toBeVisible();
  await expect(page.getByText('Checked 2 waiting claims against it: 1 verified.')).toBeVisible();
  await page.getByRole('button', { name: 'Open the review' }).click();
  await expect(page).toHaveURL(/review=claims/);
  await expect(page.getByTestId('review-tally')).toContainText('1 claim needs you');

  expect(await unsupportedCommands(page)).toEqual([]);
});
