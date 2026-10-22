import { expect, test } from '@playwright/test';

import { claimFixture, holdLessonForReview, unsupportedCommands } from './helpers/claimReviewFixtures';
import { installLearningStudioBackend } from './helpers/learningStudioBackend';
import { openStudioSection } from './helpers/learningStudioNavigation';

const NOTES = { sourceId: 'source-field-notes', title: 'Public field notes', site: null, kind: 'your_file', url: null };

test('A held lesson is one line on the course page; its review screen takes the user’s fixes one claim at a time, then resumes', async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: /Reasoning from field observations/ })).toBeVisible();
  const lesson = await holdLessonForReview(page, [
    claimFixture({ id: 'contradicted', statement: 'A comparison always proves a cause.', unitIndex: 0, verdict: 'contradicted', source: NOTES, role: 'contradicts',
      quote: 'A comparison records limits; it does not prove causes.', context: 'A careful comparison records the chosen measure and the observation period.' }),
    claimFixture({ id: 'unsupported', statement: 'Samples are always complete.', unitIndex: 1, verdict: 'unsupported' }),
  ]);
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();

  // The course page knows counts and states, never claim text.
  const row = page.locator(`#lesson-${lesson.id}`);
  await expect(row.getByText('2 claims need you')).toBeVisible();
  await expect(page.getByText('A comparison always proves a cause.')).toHaveCount(0);
  await expect(page.getByRole('region', { name: /claim review/i })).toHaveCount(0);
  await row.getByRole('button', { name: 'Review', exact: true }).click();

  // The review screen: the queue beside one claim in focus.
  await expect(page).toHaveURL(/review=claims/);
  const queue = page.getByRole('navigation', { name: 'Claims waiting for you' });
  await queue.getByLabel('Group by').selectOption('source');
  await expect(queue.getByRole('region', { name: 'No source states it · 2 claims' })).toBeVisible();
  const focus = page.getByRole('article', { name: 'Claim in focus' });
  await expect(focus.getByText('A source says otherwise.')).toBeVisible();
  const against = focus.getByRole('region', { name: 'says otherwise' });
  await expect(against.locator('mark')).toHaveText('A comparison records limits; it does not prove causes.');

  // Nothing changed yet: Resume says why it is disabled.
  const resume = page.getByRole('button', { name: 'Resume verification' });
  await expect(resume).toBeDisabled();
  await expect(resume).toHaveAccessibleDescription('Nothing changed since the last check. Resolve a claim or add a source first.');

  // Accepting a contradiction needs a note; Undo takes it back.
  await focus.getByRole('button', { name: 'Accept with a note' }).click();
  await expect(focus.getByRole('button', { name: 'Accept with this note' })).toBeDisabled();
  await focus.getByLabel('Why the claim is right (required)').fill('In this course the comparison is a controlled experiment.');
  await focus.getByRole('button', { name: 'Accept with this note' }).click();
  await expect(page.getByText('Accepted. 1 left.')).toBeVisible();
  await expect(focus.getByText('Samples are always complete.')).toBeVisible();
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByText('Undone. 2 left.')).toBeVisible();

  // A link the site refuses: the honest message and the way forward.
  await queue.getByRole('button', { name: /Samples are always complete\./ }).click();
  await focus.getByRole('button', { name: 'Add a source' }).click();
  const sheet = page.getByRole('region', { name: 'Add a source for this claim' });
  // The claim's group holds another waiting claim, so the sheet checks it too unless the user unticks it (§3.6).
  await expect(sheet.getByRole('checkbox', { name: /Also check the 1 other claim that waits/ })).toBeChecked();
  await sheet.getByLabel('Page address').fill('https://paywalled.example.com/403/sampling');
  await sheet.getByRole('button', { name: 'Add and check' }).click();
  await expect(sheet.getByText('paywalled.example.com refused the request (HTTP 403).')).toBeVisible();
  await sheet.getByRole('button', { name: 'Open it in your browser, then paste the relevant text' }).click();
  await expect(sheet.getByLabel('Where it is from')).toHaveValue('https://paywalled.example.com/403/sampling');
  await sheet.getByLabel('Text').fill('Field protocol, section 4.\nSamples are always complete.');
  await sheet.getByRole('button', { name: 'Add and check' }).click();
  await expect(page.getByText(/Verified 1 of 2 claims; 1 is not mentioned by it\./)).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: `e2e-results/lesson-claim-review-narrow-${browserName}.png` });
  await page.setViewportSize({ width: 1280, height: 800 });

  // Something changed: Resume starts a new attempt of the held job and returns to the course page.
  await expect(resume).toBeEnabled();
  await resume.click();
  await expect(page).not.toHaveURL(/review=claims/);
  await expect(page.getByRole('region', { name: 'Lesson preparation' }).getByRole('status').first()).toContainText(/Preparing|Preparation will resume/);

  expect(await unsupportedCommands(page)).toEqual([]);
});

test('Turning on the Verifier audit names the provider and what is sent, and needs the confirm button', async ({ page }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  await openStudioSection(page, 'Plan');
  const audit = page.getByRole('region', { name: 'Verifier audit' });
  const toggle = audit.getByRole('switch');
  await expect(toggle).toHaveAttribute('aria-checked', 'false');
  await expect(audit.getByText(/sent to Anthropic \(claude-fixture\) for a second check/)).toBeVisible();

  await toggle.click();
  const consent = page.getByRole('dialog', { name: 'Audit lessons with your Verifier model before publishing' });
  await expect(consent.getByText(/every factual claim extracted from it, and the exact cited passages/)).toBeVisible();
  await expect(consent.getByText('Anthropic (claude-fixture)')).toBeVisible();
  await consent.getByRole('button', { name: 'Cancel' }).click();
  await expect(toggle).toHaveAttribute('aria-checked', 'false');

  await toggle.click();
  await consent.getByRole('checkbox', { name: 'Allow the Verifier to use web search for this course' }).check();
  await consent.getByRole('button', { name: 'Turn on and send lesson text to Anthropic' }).click();
  await expect(toggle).toHaveAttribute('aria-checked', 'true');
  await expect(audit.getByText(/The Verifier may search the web for this course/)).toBeVisible();
  const saved = await page.evaluate(() => (window as unknown as { __LATTICE_LEARNING_STATE__: { verifierAudit: { on: boolean; webSearch: boolean; consentProvider: string | null } } }).__LATTICE_LEARNING_STATE__.verifierAudit);
  expect(saved).toMatchObject({ on: true, webSearch: true, consentProvider: 'anthropic' });

  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-checked', 'false');
});
