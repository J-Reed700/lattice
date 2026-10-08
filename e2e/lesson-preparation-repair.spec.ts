import { expect, test } from '@playwright/test';

import { installLearningStudioBackend } from './helpers/learningStudioBackend';

test('Completed checking gives way to visible repair progress, including when details are collapsed', async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: /Reasoning from field observations/ })).toBeVisible();
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: {
      program: { modules: Array<{ lessons: Array<{ title: string; preparation: string; blocks: unknown[]; questions: unknown[] }> }> };
      planWorkspace: { programId: string; programRevision: number; jobs: unknown[] };
    } }).__LATTICE_LEARNING_STATE__;
    const lesson = state.program.modules[0].lessons[0];
    lesson.preparation = 'outline';
    lesson.blocks = [];
    lesson.questions = [];
    state.planWorkspace.jobs = [{
      id: 'repair-job', programId: state.planWorkspace.programId, operationId: 'operation',
      kind: 'lesson_preparation', payloadSha256: 'fixture', baseRevisionNumber: state.planWorkspace.programRevision,
      status: 'running', progressCompleted: 0, progressTotal: 1,
      progressMessage: 'Correcting affected section 4 of 9 · Waiting for the model’s response',
      resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 3_600_000, startedAt: Date.now() - 3_600_000, finishedAt: null,
      activity: { phase: 'repair', phaseStartedAt: Date.now() - 600_000, lastActivityAt: Date.now() - 120_000, lastCheckpointAt: Date.now() - 120_000,
        lessonTitle: lesson.title, modelName: 'Reference checker', modelRunning: true, responseCharacters: 0, modelAttempt: 1,
        verificationPass: 43, checksCompleted: 170, checksTotal: 170, checksReused: 55, checksUnresolved: 21, modelChecksTotal: 115,
        recentSteps: [{ phase: 'evidence', startedAt: Date.now() - 900_000 }, { phase: 'repair', startedAt: Date.now() - 600_000 }],
      },
    }];
  });
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  const panel = page.getByRole('region', { name: 'Lesson preparation progress' });
  await expect(panel.getByText('Lesson not ready yet')).toBeVisible();
  await expect(panel.getByText(/Checking finished; 21 claims needed attention/)).toBeVisible();
  await expect(panel.getByText(/Correcting affected section 4 of 9/)).toBeVisible();
  await expect(panel.getByRole('progressbar')).toHaveCount(0);
  await expect(panel.getByText('Still to check')).toHaveCount(0);
  await panel.scrollIntoViewIfNeeded();
  await panel.screenshot({ path: `e2e-results/lesson-repair-progress-${browserName}.png` });
  await panel.getByRole('button', { name: 'Hide details' }).click();
  await expect(panel.getByRole('heading', { name: 'Repairing the draft' })).toBeVisible();
  await expect(panel.getByText('Waiting for model output')).toBeVisible();
  await expect(panel.getByText(/Last reported progress/)).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(panel.getByText(/Correcting affected section 4 of 9/)).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await panel.screenshot({ path: `e2e-results/lesson-repair-progress-narrow-${browserName}.png` });
  await panel.getByRole('button', { name: 'Pause lesson preparation' }).click();
  await expect(panel.getByRole('status')).toHaveText('Lesson preparation paused');
  await expect(panel.getByText('Waiting for model output')).toHaveCount(0);
  await expect(panel.getByRole('button', { name: 'Resume lesson preparation' })).toBeVisible();
});
