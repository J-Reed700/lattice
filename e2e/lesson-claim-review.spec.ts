import { expect, test, type Page } from '@playwright/test';

import { installLearningStudioBackend } from './helpers/learningStudioBackend';
import { openStudioSection } from './helpers/learningStudioNavigation';

type LearningState = {
  program: { modules: Array<{ lessons: Array<{ id: string; title: string; preparation: string; blocks: unknown[]; questions: unknown[] }> }> };
  planWorkspace: { programId: string; programRevision: number; jobs: unknown[] };
  claimReviews: Record<string, unknown>;
  claimActions: unknown[];
  unsupportedCommands: string[];
};

/** Hold the first lesson for review with a contradicted and an unsupported claim. */
async function holdLessonForReview(page: Page) {
  return page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: LearningState }).__LATTICE_LEARNING_STATE__;
    const lesson = state.program.modules[0].lessons[0];
    lesson.preparation = 'outline';
    lesson.blocks = [];
    lesson.questions = [];
    const finishedAt = Date.now() - 60_000;
    state.planWorkspace.jobs = [{
      id: 'held-job', programId: state.planWorkspace.programId, operationId: 'operation', kind: 'lesson_preparation', payloadSha256: 'fixture',
      baseRevisionNumber: state.planWorkspace.programRevision, status: 'completed', progressCompleted: 1, progressTotal: 1,
      progressMessage: 'Needs your review: 2 claims', activity: null, resultId: `needs_review:${lesson.id}`, error: null, retryOfJobId: null,
      createdAt: finishedAt - 3_600_000, startedAt: finishedAt - 3_600_000, finishedAt,
    }];
    const passage = { sourceVersionId: 'source-version-field-notes-1', title: 'Public field notes', url: null, text: 'A careful comparison records the chosen measure and the observation period.', startByte: 0, endByte: 76, retrievalKind: 'hybrid' };
    const location = (unitIndex: number, quote: string) => ({ unitIndex, unitKind: 'block', unitTitle: `Section ${unitIndex + 1} title`, field: 'body', quote, span: null });
    state.claimReviews[lesson.id] = {
      lessonId: lesson.id, programId: state.planWorkspace.programId, state: 'needs_review', revision: 3, pass: 2, stopReason: null, changedSinceStop: false,
      counts: { verified: 9, confirmed: 0, generalKnowledge: 0, queued: 2, open: 0, removed: 0 },
      teachingFindings: [],
      claims: [
        { claimId: 'contradicted', location: location(0, 'A comparison always proves a cause.'), statement: 'A comparison always proves a cause.',
          state: { state: 'queued' }, verdictKind: { kind: 'contradicted' }, reason: 'The notes say a comparison records limits; it does not prove causes.',
          evidence: [{ passage, addedBy: 'outline' }], supportingQuote: null,
          research: [{ queries: ['does comparison prove causation'], sourcesAdded: [], passagesFound: 0, at: finishedAt - 10_000 }],
          repairs: [{ attempt: 1, escalated: false, before: 'A comparison always proves a cause.', after: null, outcome: { outcome: 'rejected', reason: 'The section came back with the wrong number of paragraphs.' }, at: finishedAt - 5_000 }],
          executions: [], human: null, availableActions: ['confirm', 'edit', 'remove', 'attach_evidence', 'research_with_hint'], history: [], localCheck: null, verifierCheck: null },
        { claimId: 'unsupported', location: location(1, 'Samples are always complete.'), statement: 'Samples are always complete.',
          state: { state: 'queued' }, verdictKind: { kind: 'unsupported' }, reason: 'No saved passage states that samples are complete.',
          evidence: [], supportingQuote: null, research: [], repairs: [], executions: [], human: null,
          availableActions: ['confirm', 'edit', 'remove', 'general_knowledge', 'attach_evidence', 'research_with_hint'], history: [], localCheck: null, verifierCheck: null },
      ],
    };
    return lesson.id;
  });
}

test('A lesson held for review lists each claim with what was compared, takes the user’s fixes, and resumes', async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: /Reasoning from field observations/ })).toBeVisible();
  const lessonId = await holdLessonForReview(page);
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();

  // Not a failure: the progress area becomes the claim list.
  const progress = page.getByRole('region', { name: 'Lesson preparation progress' });
  await expect(progress.getByRole('heading', { name: 'Lesson preparation is waiting for you' })).toBeVisible();
  await expect(progress.getByText(/failed/i)).toHaveCount(0);
  const panel = progress.getByRole('region', { name: 'Claim review' });
  await expect(panel.getByRole('heading', { name: /Needs your review · 2 claims/ })).toBeVisible();
  // The lesson card names the state, not "Outline" and never "failed".
  await expect(page.locator(`#lesson-${lessonId} > button`).getByText('Needs your review · 2 claims', { exact: true })).toBeVisible();

  // Nothing changed yet: Resume says why it is disabled.
  const resume = panel.getByRole('button', { name: 'Resume verification' });
  await expect(resume).toBeDisabled();
  await expect(resume).toHaveAccessibleDescription('Nothing changed since the last pass. Resolve a claim or add evidence first.');

  // Keyboard: the claim headers are one tab stop, arrows move between them.
  const headers = panel.locator('[data-claim-header]');
  await expect(headers).toHaveCount(2);
  await headers.first().focus();
  await page.keyboard.press('ArrowDown');
  await expect(headers.nth(1)).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await expect(headers.first()).toHaveAttribute('aria-expanded', 'true');

  // The contradicted claim: verdict, reason, the passage compared, what was tried.
  const first = panel.getByRole('article').filter({ hasText: 'A comparison always proves a cause.' });
  await expect(first.getByText('A passage says otherwise.')).toBeVisible();
  await expect(first.getByText(/does not prove causes/)).toBeVisible();
  await expect(first.getByText('A careful comparison records the chosen measure and the observation period.')).toBeVisible();
  await expect(first.getByText('Saved with the course')).toBeVisible();
  await expect(first.getByText(/Searched: “does comparison prove causation”/)).toContainText('No page added.');
  await expect(first.getByText(/Repair rejected: The section came back with the wrong number of paragraphs/)).toBeVisible();
  await first.getByRole('button', { name: 'Open in reader' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  // A saved source version is not a file: the reader shows the passage as saved.
  await expect(page.getByRole('dialog').getByText('A careful comparison records the chosen measure and the observation period.').first()).toBeVisible();
  await page.keyboard.press('Escape');

  // Confirming a contradiction needs a note; Undo takes it back.
  await first.getByRole('button', { name: 'Confirm anyway (note required)' }).click();
  await expect(first.getByRole('button', { name: 'Confirm with this note' })).toBeDisabled();
  await first.getByLabel('Why the claim is right (required)').fill('In this course the comparison is a controlled experiment.');
  await first.getByRole('button', { name: 'Confirm with this note' }).click();
  await expect(panel.getByText('Claim confirmed. 1 remains.')).toBeVisible();
  await panel.getByRole('button', { name: 'Undo' }).first().click();
  await expect(panel.getByText('Undone. 2 remain.')).toBeVisible();

  // A link the site refuses: the honest message and the way forward.
  await headers.nth(1).click();
  const second = panel.getByRole('article').filter({ hasText: 'Samples are always complete.' });
  await second.getByRole('button', { name: 'Add evidence' }).click();
  await second.getByLabel('Page address').fill('https://paywalled.example.com/403/sampling');
  await second.getByRole('button', { name: 'Add and check' }).click();
  await expect(second.getByText('paywalled.example.com refused the request (HTTP 403).')).toBeVisible();
  await second.getByRole('button', { name: 'Open it in your browser, then paste the relevant text' }).click();
  await expect(second.getByLabel('Where it is from')).toHaveValue('Text from https://paywalled.example.com/403/sampling');
  await second.getByLabel('Text').fill('Every sample in the study was complete.');
  await second.getByRole('button', { name: 'Add and check' }).click();
  await expect(panel.getByText(/Verified against Text from https:\/\/paywalled.example.com\/403\/sampling/).first()).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await panel.screenshot({ path: `e2e-results/lesson-claim-review-narrow-${browserName}.png` });

  // Something changed: Resume starts a new attempt of the held job.
  await expect(resume).toBeEnabled();
  await resume.click();
  await expect(page.getByRole('region', { name: 'Lesson preparation progress' }).getByRole('status').first()).toContainText(/Preparing your lesson|Preparation will resume/);

  const unsupported = await page.evaluate(() => (window as unknown as { __LATTICE_LEARNING_STATE__: LearningState }).__LATTICE_LEARNING_STATE__.unsupportedCommands);
  expect(unsupported).toEqual([]);
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
