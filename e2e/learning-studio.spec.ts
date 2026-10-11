import { expect, test, type Page } from "@playwright/test";

import type { MockArgs } from "./fixtures/tauri";
import { openStudioSection } from "./helpers/learningStudioNavigation";
import type { CommandName } from "../src/shared/ipc/routes.generated";

import {
  installLearningStudioBackend,
  watchExternalHttpRequests,
} from "./helpers/learningStudioBackend";

const editedQuestion =
  "Which details did I choose to record before comparing observations?";
const answerSentinel =
  "CARD_ANSWER_SENTINEL: Record the measure and observation period.";
const excerptSentinel =
  "SOURCE_EXCERPT_SENTINEL: A careful comparison records the chosen measure, the observation period, and the limits of the sample.";

async function holdNextLostSaveResponse(page: Page, kind: "canvas" | "lab") {
  // Keep the failed write in flight until navigation tries to flush it. Browser
  // click/scroll timing must not decide whether autosave or navigation goes first.
  await page.evaluate((kind) => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: {
      canvasSaveLostResponsesRemaining: number;
      labDraftLostResponses: number;
      lostSaveResponseGate: Promise<void> | null;
      releaseLostSaveResponse: (() => void) | null;
    } }).__LATTICE_LEARNING_STATE__;
    if (kind === "canvas") state.canvasSaveLostResponsesRemaining = 1;
    else state.labDraftLostResponses = 1;
    state.lostSaveResponseGate = new Promise((resolve) => { state.releaseLostSaveResponse = resolve; });
  }, kind);
  return async () => page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: {
      lostSaveResponseGate: Promise<void> | null;
      releaseLostSaveResponse: (() => void) | null;
    } }).__LATTICE_LEARNING_STATE__;
    state.releaseLostSaveResponse?.();
    state.lostSaveResponseGate = null;
    state.releaseLostSaveResponse = null;
  });
}

function buttonContrast(buttons: Element[]) {
  const luminance = (color: string) => (color.match(/[\d.]+/g) ?? []).slice(0, 3).map(Number).reduce((sum, channel, index) => {
    const value = channel / 255;
    return sum + [0.2126, 0.7152, 0.0722][index] * (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  }, 0);
  return buttons.map((button) => {
    const style = getComputedStyle(button);
    const [low, high] = [luminance(style.color), luminance(style.backgroundColor)].sort((a, b) => a - b);
    return { label: button.textContent, ratio: (high + 0.05) / (low + 0.05) };
  });
}

async function openProgram(page: Page) {
  await page.goto("/studio");
  await expect(
    page.getByRole("navigation", { name: "Main navigation" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Studio", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await page
    .getByRole("button", { name: /Reasoning from field observations/ })
    .click();
  await expect(
    page.getByRole("heading", {
      name: "Reasoning from field observations",
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: "Module workspace" }),
  ).toBeVisible();
}

async function expectNoUnsupportedIpc(page: Page) {
  const unsupported = await page.evaluate(() =>
    (window as unknown as { __LATTICE_LEARNING_STATE__: { unsupportedCommands: string[] } })
      .__LATTICE_LEARNING_STATE__.unsupportedCommands,
  );
  expect(unsupported).toEqual([]);
}

test('Lesson preparation explains verification and research across navigation and narrow layouts', async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: /Reasoning from field observations/ })).toBeVisible();
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { program: { modules: Array<{ lessons: Array<{ title: string; preparation: string; blocks: unknown[]; questions: unknown[] }> }> }; planWorkspace: { programId: string; programRevision: number; jobs: unknown[] } } }).__LATTICE_LEARNING_STATE__;
    const lesson = state.program.modules[0].lessons[0];
    lesson.preparation = 'outline';
    lesson.blocks = [];
    lesson.questions = [];
    state.planWorkspace.jobs = [{
      id: 'resumable-job', programId: state.planWorkspace.programId, operationId: 'operation',
      kind: 'lesson_preparation', payloadSha256: 'fixture', baseRevisionNumber: state.planWorkspace.programRevision,
      status: 'running', progressCompleted: 0, progressTotal: 1, progressMessage: 'Checked 77 of 164 claims against saved references · 30 unchanged checks reused',
      resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 3_600_000, startedAt: Date.now() - 3_600_000, finishedAt: null,
      activity: { phase: 'evidence', phaseStartedAt: Date.now() - 600_000, lastActivityAt: Date.now() - 95_000, lastCheckpointAt: Date.now() - 96_000,
        lessonTitle: lesson.title, modelName: 'Reference checker', modelRunning: true, responseCharacters: 0, modelAttempt: 1,
        verificationPass: 2, checksCompleted: 77, checksTotal: 164, checksReused: 30, checksUnresolved: 3, modelChecksTotal: 134,
        recentSteps: [{ phase: 'review', startedAt: Date.now() - 900_000 }, { phase: 'coverage', startedAt: Date.now() - 700_000 }, { phase: 'evidence', startedAt: Date.now() - 600_000 }],
      },
    }];
  });
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  const panel = page.getByRole('region', { name: 'Lesson preparation progress' });
  await expect(panel.getByRole('heading', { name: 'Verifying factual claims' })).toBeVisible();
  await expect(panel.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '77');
  await expect(panel.getByText('30 saved checks reused. 87 of 134 model checks remaining in this pass.')).toBeVisible();
  await expect(panel.getByText(/Checkpoint saved at/)).toBeVisible();
  await expect(panel.getByText(/No new output or completed step/)).toBeVisible();
  await panel.getByText('Recent activity', { exact: true }).click();
  await panel.getByRole('status').scrollIntoViewIfNeeded();
  await page.screenshot({ path: `e2e-results/lesson-preparation-desktop-${browserName}.png` });
  const desktopViewport = page.viewportSize()!;
  await page.setViewportSize({ width: desktopViewport.width, height: 1600 });
  await panel.getByRole('status').scrollIntoViewIfNeeded();
  await page.screenshot({ path: `e2e-results/lesson-preparation-expanded-${browserName}.png` });
  await page.setViewportSize(desktopViewport);
  await panel.getByRole('button', { name: 'Hide details' }).click();
  await expect(panel.getByRole('progressbar')).toHaveCount(0);
  await expect(panel.getByRole('button', { name: 'Pause lesson preparation' })).toBeVisible();
  await panel.getByRole('button', { name: 'Show details' }).click();
  await openStudioSection(page, 'Sources');
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { planWorkspace: { jobs: Array<{ progressMessage: string; activity: { phase: string; modelRunning: boolean } }> } } }).__LATTICE_LEARNING_STATE__;
    state.planWorkspace.jobs[0].activity.phase = 'research';
    state.planWorkspace.jobs[0].activity.modelRunning = false;
    state.planWorkspace.jobs[0].progressMessage = 'Saving additional references for unresolved claims';
  });
  await openStudioSection(page, 'Lessons');
  await expect(panel.getByRole('heading', { name: 'Researching unresolved claims' })).toBeVisible();
  await expect(panel.getByText(/77 \/ 164 checked/)).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await panel.getByRole('button', { name: 'Pause lesson preparation' }).scrollIntoViewIfNeeded();
  await expect(panel.getByRole('button', { name: 'Pause lesson preparation' })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: `e2e-results/lesson-preparation-narrow-bottom-${browserName}.png` });
  await panel.getByRole('status').scrollIntoViewIfNeeded();
  await page.screenshot({ path: `e2e-results/lesson-preparation-narrow-${browserName}.png` });
  await panel.getByRole('button', { name: 'Hide details' }).click();
  await panel.getByRole('button', { name: 'Pause lesson preparation' }).click();
  await expect(panel.getByRole('status')).toHaveText('Lesson preparation paused');
  await expect(panel.getByRole('button', { name: 'Resume lesson preparation' })).toBeVisible();
  await expect(panel.getByRole('alert')).toHaveCount(0);
  await page.screenshot({ path: `e2e-results/lesson-preparation-paused-narrow-${browserName}.png` });
  await openStudioSection(page, 'Sources');
  await openStudioSection(page, 'Lessons');
  await expect(panel.getByRole('status')).toHaveText('Lesson preparation paused');
  await panel.getByRole('button', { name: 'Show details' }).click();
  await expect(panel.getByText('77 / 164 checked')).toBeVisible();
  await expect(panel.getByText(/stays paused until you choose Resume, even after reopening/)).toBeVisible();
  await page.setViewportSize(desktopViewport);
  await panel.getByRole('status').scrollIntoViewIfNeeded();
  await page.screenshot({ path: `e2e-results/lesson-preparation-paused-desktop-${browserName}.png` });
  await panel.getByRole('button', { name: 'Resume lesson preparation' }).click();
  await expect(panel.getByRole('status')).toHaveText('Preparing your lesson');
  await expect(panel.getByText('Retry queued')).toBeVisible();
  await expect(panel.getByText('77 / 164 checked')).toBeVisible();
  await expectNoUnsupportedIpc(page);
});

test("Study activity remains interactive across tabs and handles a slow reference failure", async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  await page.evaluate(() => {
    const ipc = window.__LATTICE_IPC__;
    const refresh = ipc.handlers.refresh_learning_source!;
    ipc.handlers.refresh_learning_source = (args, bridge) => new Promise((resolve, reject) => {
      (window as unknown as { __STUDY_ACTIVITY_TEST__: { fail: () => void; release: () => void } }).__STUDY_ACTIVITY_TEST__ = {
        fail: () => reject('The reference website did not respond.'),
        release: () => resolve(refresh(args, bridge)),
      };
    });
  });
  await openStudioSection(page, 'Sources');
  await page.getByRole('button', { name: 'Check for changes', exact: true }).click();
  const activity = page.getByRole('region', { name: 'Study activity' });
  await expect(activity.getByText('Checking your reference for updates')).toBeVisible();
  await expect(activity.getByText(/Elapsed/)).toBeVisible();
  await openStudioSection(page, 'Notebook');
  await activity.getByRole('button', { name: /Collapse activity/ }).click();
  await expect(activity.getByText('Checking your reference for updates')).toBeHidden();
  await activity.getByRole('button', { name: /Expand activity/ }).click();
  await activity.getByText('Task details').click();
  await expect(activity.getByText(/not live completion indicators/)).toBeVisible();
  await expect(activity.getByRole('progressbar')).toHaveCount(0);
  await page.screenshot({ path: `e2e-results/study-activity-${browserName}.png`, fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await activity.getByRole('button', { name: 'Open task', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Sources', exact: true })).toHaveAttribute('aria-selected', 'true');
  await page.evaluate(() => (window as unknown as { __STUDY_ACTIVITY_TEST__: { fail: () => void } }).__STUDY_ACTIVITY_TEST__.fail());
  await expect(activity.getByText('The reference website did not respond.')).toBeVisible();
  await expect(activity.getByText(/Took/)).toBeVisible();
  await page.screenshot({ path: `e2e-results/study-activity-narrow-${browserName}.png`, fullPage: true });
  await activity.getByRole('button', { name: 'Return to task to retry' }).click();
  await page.getByRole('button', { name: 'Check for changes', exact: true }).click();
  await expect(activity.getByText('Waiting for the result')).toBeVisible();
  await page.evaluate(() => (window as unknown as { __STUDY_ACTIVITY_TEST__: { release: () => void } }).__STUDY_ACTIVITY_TEST__.release());
  await expect(activity.getByText('Request completed')).toBeVisible();
  await activity.getByRole('button', { name: 'Clear finished activity' }).click();
  await expect(activity).toHaveCount(0);
  await expectNoUnsupportedIpc(page);
});

test("Learning Studio saved draft findings can be repaired at desktop and narrow widths", async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto("/studio");
  await expect(page.getByRole("button", { name: /Reasoning from field observations/ })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __LATTICE_OUTLINE_TEST__: { makeUnresolvedDraft(): void } }).__LATTICE_OUTLINE_TEST__.makeUnresolvedDraft());
  await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
  const review = page.getByRole("region", { name: "Outline review" });
  await expect(review.getByText("Saved draft · 1 unresolved finding")).toBeVisible();
  await expect(review.getByText("This unsupported statement is retained for review.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Accept program" })).toBeDisabled();
  await page.screenshot({ path: `e2e-results/learning-draft-${browserName}.png`, fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(review.getByRole("button", { name: "Repair remaining issues" })).toBeVisible();
  const description = await review.getByText("Completed corrections are saved. One finding remains unresolved.").boundingBox();
  const repairButton = await review.getByRole("button", { name: "Repair remaining issues" }).boundingBox();
  expect(repairButton!.y).toBeGreaterThan(description!.y + description!.height);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: `e2e-results/learning-draft-narrow-${browserName}.png`, fullPage: true });
  await review.getByRole("button", { name: "Repair remaining issues" }).click();
  await expect(review.getByText("Outline checks passed", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Accept program" })).toBeEnabled();
  await expectNoUnsupportedIpc(page);
});

test("Learning Studio citations trace saved evidence and support clean reading at desktop and narrow widths", async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  await page.getByRole("button", { name: "View lesson evidence" }).first().click();
  const evidence = page.getByRole("region", { name: "Lesson evidence" }).first();
  await expect(evidence.getByText(/14 claims checked against saved evidence/)).toBeVisible();
  await expect(evidence.getByText("A careful comparison records the chosen measure and observation period.")).toBeVisible();
  await expect(evidence.getByText("The saved reference supports this teaching claim.")).toBeVisible();
  await evidence.locator("summary").filter({ hasText: "Public field notes" }).click();
  await expect(evidence.getByText(/Saved version/)).toBeVisible();
  await expect(evidence.getByRole("link", { name: "Open original page" })).toHaveAttribute("href", /^https:/);
  await page.screenshot({ path: `e2e-results/learning-reference-mvp-${browserName}.png`, fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(evidence.getByText(/14 claims checked against saved evidence/)).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  const section = page.locator('#lesson-lesson-observation-section-0');
  await section.locator('summary').filter({ hasText: 'Evidence · 1 checked claim' }).click();
  await expect(section.getByText('A careful comparison records the chosen measure and observation period.')).toBeVisible();
  await section.locator('summary').filter({ hasText: 'Public field notes' }).click();
  await expect(section.getByText(excerptSentinel, { exact: true })).toBeVisible();
  expect(await section.getByText(excerptSentinel, { exact: true }).evaluate((element) => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  await page.screenshot({ path: `e2e-results/learning-citations-narrow-${browserName}.png`, fullPage: true });

  await page.getByRole('button', { name: 'Hide citations', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Lesson evidence' })).toHaveCount(0);
  await expect(section.locator('summary')).toHaveCount(0);
  await expect(section.getByText('A comparison begins by naming the measure and the period under observation.')).toBeVisible();
  await expect(page.getByText('View citation', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: `e2e-results/learning-clean-reading-narrow-${browserName}.png`, fullPage: true });

  await page.reload();
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  await expect(page.getByRole('button', { name: 'Show citations', exact: true })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Lesson evidence' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Show citations', exact: true }).click();
  await expect(page.getByRole('button', { name: 'View lesson evidence' }).first()).toBeVisible();
  const citationSummary = page.locator('summary:visible').filter({ hasText: 'View citation' }).first();
  const outlineCitation = citationSummary.locator('..');
  await citationSummary.click();
  await expect(outlineCitation.getByText(excerptSentinel, { exact: true })).toBeVisible();
  await expectNoUnsupportedIpc(page);
});

test('Studio makes an outline citation failure visible and retryable', async ({ page }) => {
  await installLearningStudioBackend(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/studio');
  const program = page.getByRole('button', { name: /Reasoning from field observations/ });
  await expect(program).toBeVisible();
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_LEARNING_STATE__: { outlineEvidenceFailuresRemaining: number } }).__LATTICE_LEARNING_STATE__.outlineEvidenceFailuresRemaining = 1;
  });
  await program.click();
  await expect(page.getByRole('alert')).toContainText('Outline citations could not be loaded');
  await expect(page.getByText('View citation', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Retry outline citations' }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.locator('summary:visible').filter({ hasText: 'View citation' }).first()).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expectNoUnsupportedIpc(page);
});

test("Studio text viewers survive repeated lesson and quiz navigation", async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await installLearningStudioBackend(page);
  await page.goto('/studio');
  const program = page.getByRole('button', { name: /Reasoning from field observations/ });
  await expect(program).toBeVisible();
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: {
      program: { modules: Array<{ lessons: Array<{ questions: unknown[] }> }> };
    } }).__LATTICE_LEARNING_STATE__;
    state.program.modules[0].lessons[0].questions = ['practice', 'quiz'].map((kind) => ({
      id: `navigation-${kind}`, kind, prompt: `Which detail belongs in the ${kind} record?`,
      options: ['The observation period', 'An unrelated guess'], sourceIds: [],
    }));
  });
  await program.click();
  const tabs = page.getByRole('tablist', { name: 'Program workspace', exact: true });
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 844 });
    await tabs.getByRole('tab', { name: 'Practice', exact: true }).click();
    await page.getByRole('tab', { name: 'Quick checks', exact: true }).click();
    await page.getByRole('button', { name: 'quiz', exact: true }).click();
    await expect(page.getByText('Which detail belongs in the quiz record?', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'practice', exact: true }).click();
    await expect(page.getByText('Which detail belongs in the practice record?', { exact: true })).toBeVisible();
    await tabs.getByRole('tab', { name: 'Lessons', exact: true }).click();
    await expect(page.getByRole('button', { name: 'View lesson evidence' }).first()).toBeVisible();
  }
  expect(errors).toEqual([]);
  await expectNoUnsupportedIpc(page);
});

test("Learning Studio guided exercise saves, hints, submits and revises in the lesson", async ({ page }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  const guided = page.getByRole("region", { name: "Program workspace content" }).getByLabel("Guided exercise", { exact: true });
  await guided.getByRole("button", { name: "Start guided exercise" }).click();
  const answer = guided.getByRole("textbox", { name: "Your working" });
  await answer.fill("I would compare the same measure over an equal observation period, then note the sample limits.");
  await guided.getByRole("button", { name: "Check my reasoning" }).click();
  await expect(guided.getByText("Feedback on your reasoning", { exact: true })).toBeVisible();
  await guided.getByRole("button", { name: "Give me a hint" }).click();
  await expect(guided.getByRole("button", { name: "Next hint" })).toBeVisible();
  await page.reload();
  await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
  await expect(answer).toHaveValue(/equal observation period/);
  await guided.getByRole("button", { name: "Submit guided response" }).click();
  await expect(guided.getByText("Your feedback and next revision", { exact: true })).toBeVisible();
  await guided.getByRole("button", { name: "Revise this response" }).click();
  await expect(answer).toBeEnabled();
  await expect(answer).toHaveValue(/equal observation period/);
  await expectNoUnsupportedIpc(page);
});

test("Learning Studio starting-point tasks save answers before showing actionable feedback", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installLearningStudioBackend(page);
  await openProgram(page);
  await page.getByRole("button", { name: "Check your starting point", exact: true }).click();
  await page.getByRole("button", { name: "Check my starting point", exact: true }).click();
  await page.getByRole("textbox", { name: "Your reasoning", exact: true }).fill("I would compare the measurements over the same observation period.");
  await page.getByRole("button", { name: "Get my starting-point feedback", exact: true }).click();
  const assessment = page.getByRole("region", { name: "Starting-point assessment", exact: true });
  await expect(assessment.getByText("Worth practicing", { exact: true })).toBeVisible();
  await expect(assessment.getByText("Name a consistent observation period before comparing the measurements.", { exact: true })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  await expectNoUnsupportedIpc(page);
});

async function runLearningStudioJourney(
  page: Page,
  width: number,
  height: number,
) {
  await page.setViewportSize({ width, height });
  await installLearningStudioBackend(page);
  const pageErrors: Error[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error));
  page.on("console", (message) => { if (message.type() === "error") consoleErrors.push(message.text()); });

  await openProgram(page);
  const external = watchExternalHttpRequests(page, new URL(page.url()).origin);
  const workspace = page.getByRole("navigation", { name: "Module workspace" });
  await workspace.getByRole("tab", { name: "Lessons", exact: true }).focus();
  await expect(
    workspace.getByRole("tab", { name: "Lessons", exact: true }),
  ).toBeFocused();

  await workspace
    .getByRole("tab", { name: "Notebook", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "A place to make the idea your own" }),
  ).toBeVisible();
  const notes = page.getByRole("textbox", { name: "Your notes" });
  await expect(notes).toBeVisible();
  await notes.fill(
    "Field note: compare the same measure over a stated observation period.",
  );
  // Leaving the workspace immediately flushes this edit before changing routes.
  await page.getByRole("button", { name: "All programs", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Programs", exact: true }),
  ).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              memory: { lessonNotes: Array<{ note: { content: string } }> };
              noteWrites: number;
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return {
          content: state.memory.lessonNotes[0]?.note.content,
          writes: state.noteWrites,
        };
      }),
    )
    .toEqual({
      content:
        "Field note: compare the same measure over a stated observation period.",
      writes: 1,
    });

  await page
    .getByRole("button", { name: /Reasoning from field observations/ })
    .click();
  await workspace
    .getByRole("tab", { name: "Notebook", exact: true })
    .click();
  await expect(page.getByRole("textbox", { name: "Your notes" })).toHaveText(
    "Field note: compare the same measure over a stated observation period.",
  );
  await workspace.getByRole("tab", { name: "Recall", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Keep the ideas close" }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("navigation", { name: "Module workspace" })
      .getByRole("tab", { name: "Recall", exact: true }),
  ).toHaveAttribute("aria-selected", "true");

  await page.getByLabel("Generate grounded drafts").selectOption("2");
  await page
    .getByRole("button", { name: "Generate drafts", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    "Draft generation failed: The draft service is temporarily unavailable.",
  );
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(
    page.getByText("Which details define a careful comparison, version 1?", {
      exact: true,
    }),
  ).toBeVisible();
  const draftEditor = page
    .locator("form")
    .filter({ has: page.getByLabel("Question").first() })
    .first();
  await page.getByLabel("Question").first().fill(editedQuestion);
  await draftEditor
    .getByRole("button", { name: "Save draft edits", exact: true })
    .click();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              memory: { drafts: Array<{ question: string }> };
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.memory.drafts[0]?.question;
      }),
    )
    .toBe(editedQuestion);

  const firstDraft = page.getByRole("article", {
    name: `Draft card: ${editedQuestion}`,
  });
  await firstDraft
    .getByRole("button", { name: "Accept card", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Accepted cards for this lesson" }),
  ).toBeVisible();
  const dueSection = page.locator("section").filter({
    has: page.getByRole("heading", { name: "Due cards", exact: true }),
  });
  const reviewCard = dueSection.getByRole("article", {
    name: "Current review card",
  });
  await expect(
    reviewCard.locator("p").filter({ hasText: editedQuestion }),
  ).toBeVisible();
  await expect(
    reviewCard.getByText(answerSentinel, { exact: true }),
  ).toHaveCount(0);
  await expect(
    reviewCard.getByText(excerptSentinel, { exact: true }),
  ).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              memory: {
                dueCount: number;
                studyDeck: { cards: unknown[] } | null;
              };
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return {
          dueCount: state.memory.dueCount,
          cards: state.memory.studyDeck?.cards.length ?? 0,
        };
      }),
    )
    .toEqual({ dueCount: 1, cards: 1 });

  await reviewCard
    .getByRole("button", { name: "Reveal answer", exact: true })
    .click();
  await expect(
    reviewCard.getByText(answerSentinel, { exact: true }),
  ).toBeVisible();
  await reviewCard
    .getByLabel("Toggle source excerpt for Public field notes")
    .click();
  await expect(
    reviewCard.getByText(excerptSentinel, { exact: true }),
  ).toBeVisible();
  expect(
    await reviewCard.evaluate(
      (element) => element.scrollWidth - element.clientWidth,
    ),
  ).toBeLessThanOrEqual(1);
  if (process.env.LATTICE_CAPTURE_E2E_SCREENSHOTS === "1") {
    await page.screenshot({
      path: test.info().outputPath(`learning-studio-${width}px.png`),
      fullPage: true,
    });
  }
  await dueSection.getByRole("button", { name: "good", exact: true }).click();
  await expect(dueSection.getByRole("alert")).toContainText(
    "Connection interrupted while saving review.",
  );
  await dueSection.getByRole("button", { name: "good", exact: true }).click();
  await expect(dueSection.getByRole("status")).toContainText("Review saved.");
  await expect(
    dueSection.getByText("0 due · 1 total", { exact: true }),
  ).toBeVisible();
  const reviewCalls = await page.evaluate(
    () =>
      (
        window as unknown as {
          __LATTICE_LEARNING_STATE__: {
            reviewRequests: Array<{
              reviewId: string;
              cardId: string;
              expectedReviews: number;
              rating: string;
            }>;
            memory: {
              dueCount: number;
              studyDeck: {
                cards: Array<{ reviewCount: number; intervalDays: number }>;
              } | null;
            };
          };
        }
      ).__LATTICE_LEARNING_STATE__,
  );
  // The explicit UI retry reuses the same operation ID, and the mock
  // scheduler applies it once. Mutations do not retry implicitly.
  expect(reviewCalls.reviewRequests).toHaveLength(2);
  expect(reviewCalls.reviewRequests[0]).toMatchObject({
    cardId: "accepted-draft-learning-1",
    expectedReviews: 0,
    rating: "good",
  });
  expect(reviewCalls.reviewRequests.map((request) => request.reviewId)).toEqual(
    [
      reviewCalls.reviewRequests[0].reviewId,
      reviewCalls.reviewRequests[0].reviewId,
    ],
  );
  expect(reviewCalls.memory.dueCount).toBe(0);
  expect(reviewCalls.memory.studyDeck?.cards[0]).toMatchObject({
    reviewCount: 1,
    intervalDays: 1,
  });
  const generationCalls = await page.evaluate(
    () =>
      (
        window as unknown as {
          __LATTICE_LEARNING_STATE__: {
            calls: Array<{ command: string }>;
          };
        }
      ).__LATTICE_LEARNING_STATE__.calls.filter(
        (call) =>
          call.command === "generate_learning_card_drafts",
      ).length,
  );
  expect(generationCalls).toBe(2);
  if (width === 390) {
    const overflow = await page.evaluate(() => Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) - window.innerWidth);
    expect(overflow).toBeLessThanOrEqual(1);
  }
  expect(external.externalRequests).toEqual([]);
  external.stop();
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
  await expectNoUnsupportedIpc(page);
}

test("Learning Studio persists notebook and source-grounded recall through a desktop route journey", async ({
  page,
}) => {
  await runLearningStudioJourney(page, 1440, 1000);
});

test("Learning Studio remains usable through the same journey at a narrow viewport", async ({
  page,
}) => {
  await runLearningStudioJourney(page, 390, 844);
});

test("Learning Studio programs overview and active workspace visual audit", async ({ page }) => {
  await installLearningStudioBackend(page);
  const pageErrors: Error[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  for (const viewport of [{ width: 1440, height: 1000 }, { width: 1280, height: 800 }, { width: 800, height: 600 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    await page.goto("/studio");
    const external = watchExternalHttpRequests(page, new URL(page.url()).origin);
    await expect(page.getByRole("heading", { name: "Programs", exact: true })).toBeVisible();
    await expect(page.getByRole("navigation", { name: "Main navigation" })).toBeVisible();
    await page.screenshot({ path: test.info().outputPath(`studio-programs-${viewport.width}px.png`), fullPage: true });
    await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
    await expect(page.getByRole("heading", { name: "Reasoning from field observations", exact: true })).toBeVisible();
    const workspace = page.getByRole("navigation", { name: "Module workspace" });
    await expect(page.getByRole("combobox", { name: "Module", exact: true })).toBeVisible();
    const primary = page.getByRole("tablist", { name: "Program workspace" });
    expect(await primary.evaluate((element) => element.scrollWidth - element.clientWidth)).toBeLessThanOrEqual(1);
    if (viewport.width >= 800) {
      // The lesson to resume is on the first screen: its title shows without
      // scrolling, below the module's evidence when citations are shown.
      const lessonTitle = page.locator("main article").first().getByText("Compare observations", { exact: true });
      const bounds = await lessonTitle.boundingBox();
      expect(bounds).not.toBeNull();
      expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(viewport.height);
    }
    await page.screenshot({ path: test.info().outputPath(`studio-active-workspace-${viewport.width}px.png`), fullPage: true });
    const lessonsTab = workspace.getByRole("tab", { name: "Lessons", exact: true });
    await lessonsTab.focus();
    await expect(lessonsTab).toBeFocused();
    await page.screenshot({ path: test.info().outputPath(`studio-active-lesson-${viewport.width}px.png`), fullPage: true });
    if (viewport.width === 390) {
      const overflow = await page.evaluate(() => Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) - window.innerWidth);
      expect(overflow).toBeLessThanOrEqual(1);
    }
    await page.getByRole("button", { name: "All programs", exact: true }).click();
    expect(external.externalRequests).toEqual([]);
    external.stop();
  }
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
  await expectNoUnsupportedIpc(page);
});

async function runCanvasJourney(
  page: Page,
  width: number,
  height: number,
  testExport = false,
) {
  await page.setViewportSize({ width, height });
  await installLearningStudioBackend(page);
  const pageErrors: Error[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error));
  page.on("console", (message) => { if (message.type() === "error") consoleErrors.push(message.text()); });
  await openProgram(page);

  const origin = new URL(page.url()).origin;
  const external = watchExternalHttpRequests(page, origin);
  await openStudioSection(page, "Canvas");
  const panel = page.getByRole("region", { name: "Learning canvas workspace" });
  await expect(
    panel.getByRole("heading", { name: "Learning canvas" }),
  ).toBeVisible();
  await panel
    .getByRole("button", { name: "Create canvas", exact: true })
    .click();
  await page
    .getByLabel("Canvas title", { exact: true })
    .fill("Field comparison map");
  await page
    .getByLabel("Describe your drawing or its meaning", { exact: true })
    .fill("Compare one measure across a stated observation period.");
  await panel.getByRole("button", { name: "Create", exact: true }).click();
  await expect(
    panel.getByRole("button", {
      name: "Canvas: Field comparison map",
      exact: true,
    }),
  ).toBeVisible();

  const surface = panel.locator(".learning-canvas-surface");
  await expect(surface).toBeVisible();
  await expect(surface.locator("canvas").first()).toBeVisible();
  // Use Excalidraw's own text tool so the renderer, outline, and serializer
  // all participate in the persistence round trip.
  await page.locator('label:has(input[data-testid="toolbar-text"])').click();
  await expect(page.getByTestId("toolbar-text")).toBeChecked();
  const drawingCanvas = surface.locator("canvas").last();
  const drawingCanvasBox = await drawingCanvas.boundingBox();
  expect(drawingCanvasBox).not.toBeNull();
  await drawingCanvas.click({
    position: {
      x: Math.min(300, Math.max(20, drawingCanvasBox!.width / 2)),
      y: Math.min(180, Math.max(20, drawingCanvasBox!.height / 3)),
    },
  });
  await page.keyboard.type("Observation period");
  await page.keyboard.press("Escape");
  await expect(
    panel.getByText("Observation period", { exact: true }),
  ).toBeVisible();
  await expect(
    panel.getByRole("button", { name: "Canvas element outline" }),
  ).toBeVisible();

  await expect(panel.locator('[aria-live="polite"]')).toHaveText("Saved");
  // Arm a one-time lost response after commit. The flush triggered by leaving
  // the program must keep the same request ID when Retry replays that write.
  const releaseLostSave = await holdNextLostSaveResponse(page, "canvas");
  const saveBaseline = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          canvasWorkspace: { canvases: Array<{ revision: number }> };
          canvasMutationCalls: Array<{ command: string }>;
          canvasSavedRevisions: number[];
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return {
      revision: state.canvasWorkspace.canvases[0].revision,
      callCount: state.canvasMutationCalls.filter(
        (call: { command: string }) =>
          call.command === "save_learning_canvas",
      ).length,
      savedRevisionCount: state.canvasSavedRevisions.length,
    };
  });
  await panel
    .getByLabel("Canvas title", { exact: true })
    .fill("Field comparison map");
  await panel
    .getByLabel("Describe your drawing or its meaning", { exact: true })
    .fill("A visual summary of the measure and sample limits.");
  await page.getByRole("button", { name: "All programs", exact: true }).click();
  await expect(page.getByRole("button", { name: "Saving your work…", exact: true })).toBeDisabled();
  await releaseLostSave();
  await expect(panel.getByRole("alert")).toContainText(
    "Canvas save response was lost after persistence.",
  );
  await expect(panel).toBeVisible();
  await panel.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(panel.locator('[aria-live="polite"]')).toContainText("Saved");
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              canvasMutationCalls: Array<{ command: string }>;
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.canvasMutationCalls.filter(
          (call) => call.command === "save_learning_canvas",
        ).length;
      }),
    )
    .toBe(saveBaseline.callCount + 2);

  const savedState = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          canvasWorkspace: {
            canvases: Array<{
              id: string;
              title: string;
              description: string;
              revision: number;
              sceneJson: { elements: unknown[] };
            }>;
          };
          canvasMutationCalls: Array<{
            command: string;
            request: Record<string, unknown>;
          }>;
          canvasSavedRevisions: number[];
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state;
  });
  const firstCanvas = savedState.canvasWorkspace.canvases[0];
  expect(firstCanvas).toMatchObject({
    title: "Field comparison map",
    description: "A visual summary of the measure and sample limits.",
    revision: saveBaseline.revision + 1,
  });
  expect(firstCanvas.sceneJson.elements).toHaveLength(1);
  const saveAttempts = savedState.canvasMutationCalls
    .filter((call) => call.command === "save_learning_canvas")
    .slice(saveBaseline.callCount);
  expect(saveAttempts).toHaveLength(2);
  expect(saveAttempts[1].request).toEqual(saveAttempts[0].request);
  expect(
    savedState.canvasSavedRevisions.slice(saveBaseline.savedRevisionCount),
  ).toEqual([saveBaseline.revision + 1]);

  await page.getByRole("button", { name: "All programs", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Programs", exact: true }),
  ).toBeVisible();
  // Reload resets React/query state; the command fixture rehydrates from its
  // session-backed mock database so this checks the IPC read path as well.
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Programs", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /Reasoning from field observations/ })
    .click();
  await openStudioSection(page, "Canvas");
  const reopened = page.getByRole("region", {
    name: "Learning canvas workspace",
  });
  await expect(
    reopened.getByLabel("Canvas title", { exact: true }),
  ).toHaveValue("Field comparison map");
  await expect(
    reopened.getByLabel("Describe your drawing or its meaning", {
      exact: true,
    }),
  ).toHaveValue("A visual summary of the measure and sample limits.");
  await expect(
    reopened.getByText("Observation period", { exact: true }),
  ).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: { calls: Array<{ command: string }> };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.calls.filter(
          (call) =>
            call.command === "get_learning_canvas_workspace",
        ).length;
      }),
    )
    .toBeGreaterThanOrEqual(1);

  await reopened
    .getByLabel("Checkpoint name", { exact: true })
    .fill("Baseline comparison");
  await reopened
    .getByRole("button", { name: "Create checkpoint", exact: true })
    .click();
  await expect(
    reopened.getByText("Baseline comparison", { exact: true }),
  ).toBeVisible();
  await reopened
    .getByLabel("Canvas title", { exact: true })
    .fill("Revised field comparison");
  await reopened
    .getByLabel("Describe your drawing or its meaning", { exact: true })
    .fill("Revised description for the comparison diagram.");
  await expect
    .poll(() =>
      page.evaluate(() => {
        const canvas = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              canvasWorkspace: { canvases: Array<{ revision: number }> };
            };
          }
        ).__LATTICE_LEARNING_STATE__.canvasWorkspace.canvases[0];
        return canvas.revision;
      }),
    )
    .toBe(firstCanvas.revision + 1);
  await reopened
    .getByRole("button", {
      name: "Restore checkpoint Baseline comparison",
      exact: true,
    })
    .click();
  const restoreDialog = page.getByRole("alertdialog", {
    name: /Restore “Baseline comparison”/,
  });
  await expect(restoreDialog).toBeVisible();
  await restoreDialog
    .getByRole("button", { name: "Restore checkpoint", exact: true })
    .click();
  await expect(
    reopened.getByLabel("Canvas title", { exact: true }),
  ).toHaveValue("Field comparison map");
  await expect(
    reopened.getByLabel("Describe your drawing or its meaning", {
      exact: true,
    }),
  ).toHaveValue("A visual summary of the measure and sample limits.");
  await expect(
    reopened.getByText("Observation period", { exact: true }),
  ).toBeVisible();
  await expect(
    reopened
      .locator(".learning-canvas-surface")
      .getByRole("button", { name: "Scroll back to content", exact: true }),
  ).toHaveCount(0);
  await expect(reopened.getByText(/Before restore ·/)).toBeVisible();
  const history = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          canvasWorkspace: {
            canvases: Array<{
              revision: number;
              snapshots: Array<{ name: string; title: string }>;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.canvasWorkspace.canvases[0];
  });
  expect(history.revision).toBe(firstCanvas.revision + 3);
  expect(history.snapshots).toHaveLength(2);
  expect(
    history.snapshots.find(
      (snapshot) => snapshot.name === "Baseline comparison",
    ),
  ).toMatchObject({
    title: "Field comparison map",
  });
  expect(
    history.snapshots.find((snapshot) =>
      snapshot.name.startsWith("Before restore ·"),
    ),
  ).toMatchObject({
    title: "Revised field comparison",
  });

  const exportButton = reopened.getByRole("button", {
    name: ".excalidraw",
    exact: true,
  });
  await expect(exportButton).toBeEnabled();
  await exportButton.scrollIntoViewIfNeeded();
  const jsonDownload = testExport ? page.waitForEvent("download") : undefined;
  await exportButton.click();
  if (jsonDownload) {
    expect((await jsonDownload).suggestedFilename()).toBe(
      "field-comparison-map.excalidraw",
    );
  }
  if (width === 390) {
    const overflow = await page.evaluate(
      () =>
        Math.max(
          document.documentElement.scrollWidth,
          document.body.scrollWidth,
        ) - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);
  }
  expect(external.externalRequests).toEqual([]);
  external.stop();
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
  await expectNoUnsupportedIpc(page);
  if (process.env.LATTICE_CAPTURE_E2E_SCREENSHOTS === "1") {
    await page.screenshot({
      path: test.info().outputPath(`learning-canvas-${width}px.png`),
      fullPage: true,
    });
  }
}

test("Learning Canvas saves, checkpoints, restores, and exports on desktop", async ({
  page,
}) => {
  await runCanvasJourney(page, 1440, 1000, true);
});

test("Learning Canvas remains accessible without horizontal overflow at 390px", async ({
  page,
}) => {
  await runCanvasJourney(page, 390, 844);
});

async function runSourceLibraryJourney(
  page: Page,
  width: number,
  height: number,
) {
  await page.setViewportSize({ width, height });
  await installLearningStudioBackend(page);
  const pageErrors: Error[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error));
  page.on("console", (message) => { if (message.type() === "error") consoleErrors.push(message.text()); });
  await openProgram(page);

  const origin = new URL(page.url()).origin;
  const external = watchExternalHttpRequests(page, origin);
  const moduleWorkspace = page.getByRole("navigation", {
    name: "Module workspace",
  });
  await moduleWorkspace.getByRole("tab", { name: "Sources" }).click();
  const panel = page.getByTestId("learning-sources-workspace");
  await expect(panel.getByRole("heading", { name: "Sources" })).toBeVisible();

  const initialState = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          program: { sources: Array<{ id: string }> };
          sourceWorkspace: {
            sources: Array<{
              id: string;
              activeVersionId: string | null;
              revision: number;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return {
      citedVersionId: state.program.sources[0]?.id,
      sources: state.sourceWorkspace.sources,
    };
  });
  expect(initialState.sources[0].activeVersionId).toBe(
    initialState.citedVersionId,
  );

  const search = panel.getByRole("textbox", {
    name: "Search saved source text",
  });
  await search.focus();
  await expect(search).toBeFocused();
  await search.fill("ACTIVE_SOURCE_SENTINEL");
  const initialMatch = panel.getByRole("button", {
    name: /ACTIVE_SOURCE_SENTINEL/,
  });
  await expect(initialMatch).toBeVisible();
  await initialMatch.click();
  await expect(
    panel
      .getByRole("article")
      .getByText("ACTIVE_SOURCE_SENTINEL", { exact: false }),
  ).toBeVisible();
  await search.fill("");

  const addMaterial = panel.getByRole("button", {
    name: "Add material",
    exact: true,
  });
  await addMaterial.focus();
  await expect(addMaterial).toBeFocused();
  await page.keyboard.press("Enter");
  const addDialog = page.getByRole("dialog", { name: "Bring in a source" });
  await expect(addDialog).toBeVisible();
  await expect(
    addDialog.getByRole("button", { name: "Web page" }),
  ).toHaveAttribute("aria-pressed", "true");
  const closeAddDialog = addDialog.getByRole("button", {
    name: "Close add material",
  });
  const saveSource = addDialog.getByRole("button", { name: "Save source" });
  await closeAddDialog.focus();
  await expect(closeAddDialog).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(saveSource).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(closeAddDialog).toBeFocused();
  await addDialog
    .getByLabel("Web address", { exact: true })
    .fill("https://example.org/supplementary-notes");
  await addDialog.locator("#new-source-policy").selectOption("manual");
  await saveSource.click();
  await expect(addDialog).not.toBeVisible();
  await expect(addMaterial).toBeFocused();
  const supplementaryRow = panel.getByRole("button", {
    name: /Supplementary field notes/,
  });
  await expect(supplementaryRow).toBeVisible();
  await supplementaryRow.click();

  const addedSourceId = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{ id: string; requestedUrl: string | null }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find(
      (item) => item.requestedUrl === "https://example.org/supplementary-notes",
    )?.id;
  });
  expect(addedSourceId).toBeTruthy();

  const refreshButton = panel.getByRole("button", {
    name: "Check for changes",
  });
  const unchangedRevision = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{ id: string; revision: number }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId)
      ?.revision;
  }, addedSourceId);
  await refreshButton.click();
  await expect(
    panel.getByText(/Last check found the same saved text/),
  ).toBeVisible();
  const afterUnchanged = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              revision: number;
              versions: unknown[];
              latestCheck: { status: string } | null;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const source = state.sourceWorkspace.sources.find(
      (item) => item.id === sourceId,
    );
    return source;
  }, addedSourceId);
  expect(afterUnchanged).toMatchObject({
    revision: unchangedRevision,
    latestCheck: { status: "unchanged" },
  });
  expect(afterUnchanged?.versions).toHaveLength(1);

  await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          nextSourceRefreshOutcome: "unchanged" | "changed" | "failed";
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    state.nextSourceRefreshOutcome = "failed";
  });
  await refreshButton.click();
  await expect(panel.getByText(/Last check failed/)).toBeVisible();
  const afterFailed = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              revision: number;
              latestCheck: { status: string } | null;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(afterFailed).toMatchObject({
    revision: unchangedRevision,
    latestCheck: { status: "failed" },
  });

  const changedBaseline = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          nextSourceRefreshOutcome: "unchanged" | "changed" | "failed";
          sourceLostResponsesRemaining: number;
          sourceWorkspace: {
            sources: Array<{ id: string; revision: number }>;
          };
          sourceMutationCalls: Array<{
            command: string;
            request: Record<string, unknown>;
          }>;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    state.nextSourceRefreshOutcome = "changed";
    state.sourceLostResponsesRemaining = 1;
    return {
      revision: state.sourceWorkspace.sources.find(
        (item) => item.id === sourceId,
      )?.revision,
      refreshCallCount: state.sourceMutationCalls.filter(
        (call) => call.command === "refresh_learning_source",
      ).length,
    };
  }, addedSourceId);
  await refreshButton.click();
  await expect(panel.getByRole("alert")).toContainText(
    "Source update response was lost after persistence.",
  );
  await panel.getByRole("button", { name: "Retry same operation" }).click();
  const availableUpdate = panel.getByRole("region", {
    name: "Available update",
  });
  await expect(availableUpdate).toBeVisible();
  const changedState = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              revision: number;
              activeVersionId: string | null;
              pendingVersionId: string | null;
              versions: Array<{ id: string; versionNumber: number }>;
              checks: Array<{ status: string }>;
            }>;
          };
          sourceMutationCalls: Array<{
            command: string;
            request: Record<string, unknown>;
          }>;
          sourceAppliedEffects: Record<string, number>;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const source = state.sourceWorkspace.sources.find(
      (item) => item.id === sourceId,
    );
    const refreshCalls = state.sourceMutationCalls.filter(
      (call) =>
        call.command === "refresh_learning_source" &&
        call.request.sourceId === sourceId,
    );
    return {
      source,
      refreshCalls,
      sourceAppliedEffects: state.sourceAppliedEffects,
    };
  }, addedSourceId);
  expect(changedState.source).toMatchObject({
    revision: (changedBaseline.revision ?? 0) + 1,
    pendingVersion: { versionNumber: 2 },
    latestCheck: { status: "update_available" },
  });
  expect(changedState.source?.activeVersionId).not.toBe(
    changedState.source?.pendingVersionId,
  );
  expect(changedState.source?.versions).toHaveLength(2);
  const changedReplay = changedState.refreshCalls.slice(
    changedBaseline.refreshCallCount,
  );
  expect(changedReplay).toHaveLength(2);
  expect(changedReplay[1].request).toEqual(changedReplay[0].request);
  const changedOperationId = String(changedReplay[0].request.operationId);
  expect(changedState.sourceAppliedEffects[changedOperationId]).toBe(1);

  // A second check with the same pending digest appends check history but does
  // not duplicate the immutable version or increment the pointer revision.
  await refreshButton.click();
  await expect
    .poll(() =>
      page.evaluate((sourceId) => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              sourceWorkspace: {
                sources: Array<{
                  id: string;
                  checks: unknown[];
                  versions: unknown[];
                  revision: number;
                }>;
              };
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.sourceWorkspace.sources.find(
          (item) => item.id === sourceId,
        );
      }, addedSourceId),
    )
    .toMatchObject({ revision: changedBaseline.revision! + 1 });
  const pendingVersionId = changedState.source?.pendingVersionId;
  expect(pendingVersionId).toBeTruthy();
  const reusedPending = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              checks: Array<{ status: string }>;
              versions: Array<{ id: string }>;
              pendingVersionId: string | null;
              revision: number;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(reusedPending).toMatchObject({
    revision: changedBaseline.revision! + 1,
    pendingVersionId,
    checks: [
      { status: "update_available" },
      { status: "update_available" },
      { status: "failed" },
      { status: "unchanged" },
    ],
  });
  expect(reusedPending?.versions).toHaveLength(2);

  // A later check that matches the active digest clears the stale pending
  // pointer while retaining that immutable edition in history. A subsequent
  // changed check should reactivate the same stored version rather than
  // creating a duplicate.
  await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          nextSourceRefreshOutcome: "unchanged" | "changed" | "failed";
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    state.nextSourceRefreshOutcome = "unchanged";
  });
  await refreshButton.click();
  await expect(panel.getByText(/Last check found the same saved text/)).toBeVisible();
  const clearedPending = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              revision: number;
              pendingVersionId: string | null;
              versions: Array<{ id: string }>;
              latestCheck: { status: string; pendingVersionId: string | null } | null;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(clearedPending).toMatchObject({
    revision: changedBaseline.revision! + 2,
    pendingVersionId: null,
    latestCheck: { status: "unchanged", pendingVersionId: null },
  });
  expect(clearedPending?.versions).toHaveLength(2);
  expect(clearedPending?.versions.some((version) => version.id === pendingVersionId)).toBe(true);

  await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          nextSourceRefreshOutcome: "unchanged" | "changed" | "failed";
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    state.nextSourceRefreshOutcome = "changed";
  });
  await refreshButton.click();
  await expect(panel.getByRole("region", { name: "Available update" })).toBeVisible();
  const reusedHistoricalPending = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              revision: number;
              pendingVersionId: string | null;
              versions: Array<{ id: string }>;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(reusedHistoricalPending).toMatchObject({
    revision: changedBaseline.revision! + 3,
    pendingVersionId,
  });
  expect(reusedHistoricalPending?.versions).toHaveLength(2);

  await search.fill("UPDATED_SOURCE_SENTINEL");
  const updatedMatch = panel.getByRole("button", {
    name: /UPDATED_SOURCE_SENTINEL/,
  });
  await expect(updatedMatch).toBeVisible();
  await updatedMatch.click();
  await expect(
    panel
      .getByRole("article")
      .getByText("UPDATED_SOURCE_SENTINEL", { exact: false }),
  ).toBeVisible();

  // Pending is durable across a page reload and does not silently replace the
  // active version. The original and pending edition remain independently read.
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Programs", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /Reasoning from field observations/ })
    .click();
  await moduleWorkspace.getByRole("tab", { name: "Sources" }).click();
  const reopened = page.getByTestId("learning-sources-workspace");
  await reopened
    .getByRole("button", { name: /Supplementary field notes/ })
    .click();
  const reopenedWorkspace = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              activeVersionId: string | null;
              pendingVersionId: string | null;
              revision: number;
              versions: Array<{ id: string; versionNumber: number }>;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(reopenedWorkspace).toMatchObject({
    activeVersionId: changedState.source?.activeVersionId,
    pendingVersionId,
  });
  await reopened.getByRole("button", { name: "Review update" }).click();
  await expect(
    reopened
      .getByRole("article")
      .getByText("UPDATED_SOURCE_SENTINEL", { exact: false }),
  ).toBeVisible();
  await reopened.getByRole("button", { name: "Adopt this version" }).click();
  await expect(
    reopened
      .getByRole("article")
      .getByText("UPDATED_SOURCE_SENTINEL", { exact: false }),
  ).toBeVisible();
  const adoptedState = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              activeVersionId: string | null;
              pendingVersionId: string | null;
              revision: number;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(adoptedState).toMatchObject({
    activeVersionId: pendingVersionId,
    pendingVersionId: null,
  });

  // Simulate another writer winning after the reader loaded its revision.
  await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{ id: string; revision: number }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const source = state.sourceWorkspace.sources.find(
      (item) => item.id === sourceId,
    );
    if (!source) throw new Error("Added source fixture missing");
    source.revision += 1;
  }, addedSourceId);
  await reopened
    .getByLabel("Freshness policy", { exact: true })
    .selectOption("before_use");
  await expect(reopened.getByRole("alert")).toContainText("changed elsewhere");
  const workspaceReadsBeforeConflictReload = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: { calls: Array<{ command: string }> };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.calls.filter(
      (call) =>
        call.command === "get_learning_source_workspace",
    ).length;
  });
  await reopened.getByRole("button", { name: "Reload sources" }).click();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: { calls: Array<{ command: string }> };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.calls.filter(
          (call) =>
            call.command === "get_learning_source_workspace",
        ).length;
      }),
    )
    .toBeGreaterThan(workspaceReadsBeforeConflictReload);
  await expect(reopened.getByRole("alert")).toHaveCount(0);
  await page.waitForTimeout(100);
  await reopened
    .getByLabel("Freshness policy", { exact: true })
    .selectOption("before_use");
  await expect(
    reopened.getByLabel("Freshness policy", { exact: true }),
  ).toHaveValue("before_use");
  const policyState = await page.evaluate((sourceId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          sourceWorkspace: {
            sources: Array<{
              id: string;
              freshnessPolicy: string;
              revision: number;
            }>;
          };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.sourceWorkspace.sources.find((item) => item.id === sourceId);
  }, addedSourceId);
  expect(policyState).toMatchObject({
    freshnessPolicy: "before_use",
    revision: (adoptedState?.revision ?? 0) + 2,
  });

  // Restart-like reload must still open the exact retired edition by immutable
  // version ID rather than showing whichever version is currently active.
  const oldVersionId = reopenedWorkspace?.versions.find(
    (version) => version.versionNumber === 1,
  )?.id;
  expect(oldVersionId).toBeTruthy();
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Programs", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /Reasoning from field observations/ })
    .click();
  await moduleWorkspace.getByRole("tab", { name: "Sources" }).click();
  const afterPolicyReload = page.getByTestId("learning-sources-workspace");
  await afterPolicyReload
    .getByRole("button", { name: /Supplementary field notes/ })
    .click();
  await afterPolicyReload
    .getByLabel("Source version history", { exact: true })
    .selectOption(oldVersionId!);
  await expect(
    afterPolicyReload
      .getByRole("article")
      .getByText("ADDED_SOURCE_SENTINEL", { exact: false }),
  ).toBeVisible();

  const otherProgramSources = await page.evaluate(async () => {
    const invoke = window.__LATTICE_IPC__.invoke;
    return invoke("get_learning_source_workspace", {
      id: "program-learning-other",
    });
  });
  expect(otherProgramSources).toEqual({
    programId: "program-learning-other",
    sources: [],
  });
  const otherProgramSearch = await page.evaluate(async () => {
    const invoke = window.__LATTICE_IPC__.invoke;
    return invoke("search_learning_sources", {
      request: {
        programId: "program-learning-other",
        query: "UPDATED_SOURCE_SENTINEL",
        limit: 10,
      },
    });
  });
  expect(otherProgramSearch).toEqual([]);

  if (width === 390) {
    const overflow = await page.evaluate(
      () =>
        Math.max(
          document.documentElement.scrollWidth,
          document.body.scrollWidth,
        ) - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);
  }
  expect(external.externalRequests).toEqual([]);
  external.stop();
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
  await expectNoUnsupportedIpc(page);
  if (process.env.LATTICE_CAPTURE_E2E_SCREENSHOTS === "1") {
    await page.screenshot({
      path: test.info().outputPath(`learning-sources-${width}px.png`),
      fullPage: true,
    });
  }
}

test("Learning Sources library versions and adopts sources on desktop", async ({
  page,
}) => {
  await runSourceLibraryJourney(page, 1440, 1000);
});

test("Learning Sources library remains accessible without overflow at 390px", async ({
  page,
}) => {
  await runSourceLibraryJourney(page, 390, 844);
});

async function fixtureInvokeError(page: Page, command: CommandName, args: MockArgs) {
  return page.evaluate(async ({ command, args }) => {
    const invoke = window.__LATTICE_IPC__.invoke;
    try {
      await invoke(command, args);
      return null;
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  }, { command, args });
}

async function runPracticeWorkbenchJourney(
  page: Page,
  width: number,
  height: number,
) {
  await page.setViewportSize({ width, height });
  await installLearningStudioBackend(page);
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  await openProgram(page);
  const origin = new URL(page.url()).origin;
  const external = watchExternalHttpRequests(page, origin);
  const workspaceNav = page.getByRole("navigation", {
    name: "Module workspace",
  });
  await openStudioSection(page, "Written practice");
  const workbenchTab = page.getByRole("tablist", { name: "Practice activities" }).getByRole("tab", {
    name: "Written practice",
    exact: true,
  });
  await workbenchTab.focus();
  await expect(workbenchTab).toBeFocused();
  await workbenchTab.click();
  const workbench = page.getByRole("region", {
    name: "Grounded Practice Workbench",
  });
  await expect(
    workbench.getByRole("heading", { name: "Make your thinking visible" }),
  ).toBeVisible();

  const practiceChoice = workbench.getByRole("radio", { name: /Practice/ });
  await practiceChoice.focus();
  await expect(practiceChoice).toBeFocused();
  await practiceChoice.check();
  await workbench.getByRole("button", { name: "Start an attempt" }).click();
  const response = workbench.getByRole("textbox", { name: "Your response" });
  await expect(response).toBeVisible();
  await expect(
    workbench.getByRole("heading", { name: "Respond to the problem" }),
  ).toBeVisible();

  const started = await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceWorkspace: {
            sessions: Array<{
              id: string;
              mode: string;
              sourceVersionIds: string[];
              revision: number;
            }>;
          };
          practiceSessions: Map<string, { rubric: unknown[] }>;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const summary = state.practiceWorkspace.sessions[0];
    return {
      summary,
      session: summary
        ? state.practiceSessions.get(summary.id)
        : undefined,
    };
  });
  expect(started.summary).toMatchObject({
    mode: "practice",
    revision: 0,
    sourceVersionIds: ["source-version-field-notes-1"],
  });
  expect(started.session?.rubric).toHaveLength(4);

  // The first autosave commits but loses its response. Explicit Retry must
  // replay the byte-for-byte request with the same operation ID.
  await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceLostResponsesRemaining: number;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    state.practiceLostResponsesRemaining = 1;
  });
  const firstText =
    "I would name one measure, hold the observation period steady, and compare the same cases. The sample limits what I can infer.";
  await response.fill(firstText);
  await expect(workbench.getByRole("alert")).toContainText(
    "Practice mutation response was lost after persistence.",
  );
  await workbench.getByRole("button", { name: "Retry save" }).click();
  await expect(
    workbench.getByRole("status").filter({ hasText: "Saved" }),
  ).toBeVisible();
  const firstSave = await page.evaluate((sessionId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceMutationCalls: Array<{
            command: string;
            request: Record<string, unknown>;
          }>;
          practiceAppliedEffects: Record<string, number>;
          practiceSessions: Map<
            string,
            { artifact: { revision: number; text: string }; summary: { revision: number } }
          >;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const calls = state.practiceMutationCalls.filter(
      (item) =>
        item.command === "save_learning_practice_artifact" &&
        item.request.sessionId === sessionId,
    );
    return {
      calls,
      effects: state.practiceAppliedEffects,
      session: state.practiceSessions.get(sessionId),
    };
  }, started.summary!.id);
  expect(firstSave.calls).toHaveLength(2);
  expect(firstSave.calls[1].request).toEqual(firstSave.calls[0].request);
  const firstSaveOperation = String(firstSave.calls[0].request.operationId);
  expect(firstSave.effects[firstSaveOperation]).toBe(1);
  expect(firstSave.session).toMatchObject({
    artifact: { revision: 1, text: firstText },
    summary: { revision: 1, artifactRevision: 1 },
  });

  const changedText = `${firstText}\nA later response adds a concrete example.`;
  await response.fill(changedText);
  await expect
    .poll(() =>
      page.evaluate((sessionId) => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              practiceSessions: Map<string, { artifact: { text: string } }>;
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.practiceSessions.get(sessionId)?.artifact.text;
      }, started.summary!.id),
    )
    .toBe(changedText);
  await expect(
    workbench.getByRole("status").filter({ hasText: "Saved" }),
  ).toBeVisible();
  const changedSave = await page.evaluate((sessionId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceMutationCalls: Array<{
            command: string;
            request: Record<string, unknown>;
          }>;
          practiceAppliedEffects: Record<string, number>;
          practiceSessions: Map<
            string,
            { artifact: { revision: number; text: string }; summary: { revision: number } }
          >;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const calls = state.practiceMutationCalls.filter(
      (item) =>
        item.command === "save_learning_practice_artifact" &&
        item.request.sessionId === sessionId,
    );
    return {
      calls,
      effects: state.practiceAppliedEffects,
      session: state.practiceSessions.get(sessionId),
    };
  }, started.summary!.id);
  expect(changedSave.calls).toHaveLength(3);
  const changedSaveRequest = changedSave.calls[2].request;
  expect(changedSaveRequest.operationId).not.toBe(firstSaveOperation);
  expect(changedSaveRequest.text).toBe(changedText);
  expect(changedSave.effects[String(changedSaveRequest.operationId)]).toBe(1);
  expect(changedSave.session).toMatchObject({
    artifact: { revision: 2, text: changedText },
    summary: { revision: 2, artifactRevision: 2 },
  });

  // Attempts freeze the original source edition. Adopt an update in the actual
  // Sources UI, then verify this attempt still points at the retired version.
  await workspaceNav.getByRole("tab", { name: "Sources", exact: true }).click();
  const sourcesPanel = page.getByTestId("learning-sources-workspace");
  await expect(sourcesPanel).toBeVisible();
  await page.evaluate(() => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          nextSourceRefreshOutcome: "unchanged" | "changed" | "failed";
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    state.nextSourceRefreshOutcome = "changed";
  });
  await sourcesPanel.getByRole("button", { name: "Check for changes" }).click();
  await sourcesPanel.getByRole("button", { name: "Review update" }).click();
  await expect(
    sourcesPanel
      .getByRole("article")
      .getByText("UPDATED_SOURCE_SENTINEL", { exact: false }),
  ).toBeVisible();
  await sourcesPanel.getByRole("button", { name: "Adopt this version" }).click();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              sourceWorkspace: {
                sources: Array<{
                  id: string;
                  activeVersionId: string | null;
                  versions: Array<{ versionNumber: number }>;
                }>;
              };
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.sourceWorkspace.sources.find(
          (source) => source.id === "logical-source-field-notes",
        );
      }),
    )
    .toMatchObject({ activeVersionId: "source-version-logical-source-field-notes-2" });
  const afterAdoption = await page.evaluate((sessionId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceSessions: Map<string, { summary: { sourceVersionIds: string[] } }>;
          sourceWorkspace: { sources: Array<{ activeVersionId: string | null }> };
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return {
      frozen: state.practiceSessions.get(sessionId)?.summary.sourceVersionIds,
      active: state.sourceWorkspace.sources[0]?.activeVersionId,
    };
  }, started.summary!.id);
  expect(afterAdoption).toEqual({
    frozen: ["source-version-field-notes-1"],
    active: "source-version-logical-source-field-notes-2",
  });

  await openStudioSection(page, "Written practice");
  await expect(workbench.getByRole("textbox", { name: "Your response" })).toHaveValue(changedText);
  const modeGroup = workbench.getByRole("group", { name: "Session mode" });
  const demonstrate = modeGroup.getByRole("button", { name: "Demonstrate" });
  await demonstrate.click();
  await expect(demonstrate).toHaveAttribute("aria-pressed", "true");
  await expect(workbench.getByRole("button", { name: "Ask tutor" })).toBeDisabled();
  await expect(workbench.getByRole("button", { name: "Request critique" })).toBeDisabled();
  await expect(workbench.getByRole("button", { name: /Orienting question/ })).toBeDisabled();
  await expect(workbench.getByRole("button", { name: /Review reveal choice/ })).toHaveCount(0);

  const demoDenials = await page.evaluate(async (sessionId) => {
    const invoke = window.__LATTICE_IPC__.invoke;
    const revision = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceSessions: Map<string, { summary: { revision: number } }>;
        };
      }
    ).__LATTICE_LEARNING_STATE__.practiceSessions.get(sessionId)!.summary.revision;
    const base = {
      operationId: crypto.randomUUID(),
      programId: "program-learning-1",
      sessionId,
      expectedRevision: revision,
    };
    const attempt = async (command: string, request: Record<string, unknown>) => {
      try {
        await invoke(command, { request });
        return null;
      } catch (error) {
        return error instanceof Error ? error.message : String(error);
      }
    };
    return Promise.all([
      attempt("request_learning_tutor_response", {
        ...base,
        requestKind: "hint",
        prompt: "Give an orienting hint.",
        hintLevel: "orienting_question",
      }),
      attempt("open_learning_practice_source", {
        ...base,
        sourceId: "logical-source-field-notes",
        versionId: "source-version-field-notes-1",
      }),
      attempt("reveal_learning_practice_solution", base),
    ]);
  }, started.summary!.id);
  expect(demoDenials).toEqual([
    "This aid is unavailable in Demonstrate mode.",
    "Source access is unavailable in Demonstrate mode.",
    "Solution reveal is unavailable in Demonstrate mode.",
  ]);

  const exploreRadio = modeGroup.getByRole("button", { name: "Explore" });
  await exploreRadio.click();
  await expect(exploreRadio).toHaveAttribute("aria-pressed", "true");
  const frozenSource = workbench.getByRole("button", { name: /Public field notes.*Version 1/ });
  await frozenSource.click();
  await expect(workbench.getByText("Immutable source version", { exact: true })).toBeVisible();
  await expect(workbench.getByRole("region", { name: "Public field notes" }).getByText("ACTIVE_SOURCE_SENTINEL", { exact: false })).toBeVisible();

  const hintLevels = [
    ["Orienting question", "orienting_question"],
    ["Concept or source", "concept_or_source"],
    ["Partial strategy", "partial_strategy"],
    ["Worked explanation", "worked_explanation"],
  ] as const;
  for (let index = 0; index < hintLevels.length; index += 1) {
    const [label] = hintLevels[index];
    await workbench.getByRole("button", { name: new RegExp(label) }).click();
    await expect
      .poll(() =>
        page.evaluate((sessionId) => {
          const state = (
            window as unknown as {
              __LATTICE_LEARNING_STATE__: {
                practiceSessions: Map<string, { tutorTurns: unknown[] }>;
              };
            }
          ).__LATTICE_LEARNING_STATE__;
          return state.practiceSessions.get(sessionId)?.tutorTurns.length;
        }, started.summary!.id),
      )
      .toBe(index + 1);
  }
  const citationButton = workbench.getByRole("button", {
    name: /Saved source quote · exact version/,
  }).first();
  await expect(citationButton).toBeVisible();
  await citationButton.click();
  const sourceReader = workbench.getByRole("region", { name: "Public field notes" });
  await expect(sourceReader.getByText("Immutable source version", { exact: true })).toBeVisible();
  await expect(sourceReader.getByText("Version 1 · saved", { exact: false })).toBeVisible();
  await expect(sourceReader.getByText("Record the same measure across the observation period.", { exact: false })).toBeVisible();

  const tutorPrompt = workbench.getByRole("textbox", {
    name: "Ask a question or request critique",
  });
  await tutorPrompt.fill("How should I state a limit on the sample?");
  await workbench.getByRole("button", { name: "Ask tutor" }).click();
  const proposalSection = workbench
    .locator("section")
    .filter({ hasText: "Tutor proposals" });
  await expect(proposalSection.getByRole("button", { name: "Accept idea" })).toHaveCount(2);
  const misconception = proposalSection
    .locator("article")
    .filter({ hasText: "Possible misconception" });
  const followUp = proposalSection
    .locator("article")
    .filter({ hasText: "Follow-up" });
  await misconception.getByRole("button", { name: "Accept idea" }).click();
  await expect(misconception.getByText("accepted", { exact: true })).toBeVisible();
  await followUp.getByRole("button", { name: "Not useful" }).click();
  await expect(followUp.getByText("rejected", { exact: true })).toBeVisible();

  await workbench.getByRole("button", { name: "Review reveal choice" }).click();
  const revealDialog = page.getByRole("dialog", {
    name: "Reveal the worked solution?",
  });
  await expect(revealDialog).toBeVisible();
  const keepWorking = revealDialog.getByRole("button", { name: "Keep working" });
  await expect(keepWorking).toBeFocused();
  await keepWorking.press("Tab");
  await expect(revealDialog.getByRole("button", { name: "Reveal solution" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(revealDialog).toHaveCount(0);
  await workbench.getByRole("button", { name: "Review reveal choice" }).click();
  await page.getByRole("dialog", { name: "Reveal the worked solution?" })
    .getByRole("button", { name: "Reveal solution" }).click();
  await expect(workbench.getByRole("heading", { name: "Worked solution revealed" })).toBeVisible();
  await expect(workbench.getByText("Revealed worked solution", { exact: true })).toBeVisible();

  const submittedText = `${changedText}\nI would disclose which cases are missing before drawing a broader conclusion.`;
  await response.fill(submittedText);
  await expect
    .poll(() =>
      page.evaluate((sessionId) => {
        const state = (
          window as unknown as {
            __LATTICE_LEARNING_STATE__: {
              practiceSessions: Map<string, { artifact: { text: string } }>;
            };
          }
        ).__LATTICE_LEARNING_STATE__;
        return state.practiceSessions.get(sessionId)?.artifact.text;
      }, started.summary!.id),
    )
    .toBe(submittedText);
  await expect(workbench.getByRole("status").filter({ hasText: "Saved" })).toBeVisible();
  await workbench.getByRole("button", { name: "Submit response" }).click();
  await expect(workbench.getByRole("heading", { name: /Feedback on revision/ })).toBeVisible();
  const submitted = await page.evaluate((sessionId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceSessions: Map<string, {
            summary: { status: string; mode: string; revision: number; sourceVersionIds: string[] };
            artifact: { revision: number; text: string };
            result: { artifactRevision: number; artifactText: string; evidence: Array<{ dimension: string }>; assistance: Array<{ kind: string }> } | null;
            proposals: Array<{ status: string }>;
          }>;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    return state.practiceSessions.get(sessionId);
  }, started.summary!.id);
  expect(submitted).toMatchObject({
    summary: { status: "submitted", mode: "explore", sourceVersionIds: ["source-version-field-notes-1"] },
    result: { artifactText: submittedText, evidence: [
      { dimension: "recall" },
      { dimension: "explanation" },
      { dimension: "application" },
      { dimension: "transfer" },
    ] },
    proposals: [{ status: "accepted" }, { status: "rejected" }],
  });
  expect(submitted?.result?.assistance.map((event) => event.kind)).toEqual(
    expect.arrayContaining(["source_opened", "hint", "tutor_response", "mode_changed", "solution_revealed"]),
  );

  await page.reload();
  await expect(page.getByRole("heading", { name: "Programs", exact: true })).toBeVisible();
  await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
  await openStudioSection(page, "Written practice");
  const reopenedWorkbench = page.getByRole("region", { name: "Grounded Practice Workbench" });
  await expect(reopenedWorkbench.getByRole("heading", { name: /Feedback on revision/ })).toBeVisible();
  await expect(reopenedWorkbench.getByRole("textbox", { name: "Your response" })).toHaveValue(submittedText);
  await expect(reopenedWorkbench.getByRole("region", { name: "Saved versions for this attempt" }).getByText("ACTIVE_SOURCE_SENTINEL", { exact: false })).toBeVisible();
  await expect(reopenedWorkbench.getByText("source-version-field-notes-1", { exact: false })).toHaveCount(0);
  const savedSourceCitation = reopenedWorkbench.getByRole("button", { name: /Saved source quote · exact version/ }).first();
  await savedSourceCitation.click();
  await expect(reopenedWorkbench.getByRole("region", { name: "Public field notes" }).getByText("Immutable source version", { exact: true })).toBeVisible();
  const postSubmitCalls = await page.evaluate((sessionId) => {
    const state = (
      window as unknown as {
        __LATTICE_LEARNING_STATE__: {
          practiceMutationCalls: Array<{ command: string; request: Record<string, unknown> }>;
          calls: Array<{ command: string; args: unknown }>;
          practiceSessions: Map<string, { summary: { revision: number } }>;
        };
      }
    ).__LATTICE_LEARNING_STATE__;
    const op = state.practiceSessions.get(sessionId)!.summary.revision;
    return {
      revision: op,
      opens: state.practiceMutationCalls.filter((item) => item.command === "open_learning_practice_source" && item.request.sessionId === sessionId).length,
      versionReads: state.calls.filter((item) => item.command === "get_learning_source_version").length,
    };
  }, started.summary!.id);
  expect(postSubmitCalls.versionReads).toBeGreaterThan(0);
  const immutableSaveError = await fixtureInvokeError(page, "save_learning_practice_artifact", {
    request: {
      operationId: crypto.randomUUID(),
      programId: "program-learning-1",
      sessionId: started.summary!.id,
      expectedRevision: postSubmitCalls.revision,
      text: "This submitted response must not change.",
    },
  });
  expect(immutableSaveError).toBe("Submitted practice attempts are immutable.");

  if (width === 390) {
    const overflow = await page.evaluate(
      () =>
        Math.max(
          document.documentElement.scrollWidth,
          document.body.scrollWidth,
        ) - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);
  }
  expect(external.externalRequests).toEqual([]);
  external.stop();
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
  await expectNoUnsupportedIpc(page);
  if (process.env.LATTICE_CAPTURE_E2E_SCREENSHOTS === "1") {
    await page.screenshot({
      path: test.info().outputPath(`learning-practice-workbench-${width}px.png`),
      fullPage: true,
    });
  }
}

test("Learning Practice Workbench freezes and grades a grounded attempt on desktop", async ({
  page,
}) => {
  await runPracticeWorkbenchJourney(page, 1440, 1000);
});

test("Learning Practice Workbench remains accessible without overflow at 390px", async ({
  page,
}) => {
  await runPracticeWorkbenchJourney(page, 390, 844);
});

async function runExtendedStudioJourney(page: Page, width: number, height: number) {
  await page.setViewportSize({ width, height });
  await installLearningStudioBackend(page);
  const pageErrors: Error[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  await openProgram(page);
  const origin = new URL(page.url()).origin;
  const external = watchExternalHttpRequests(page, origin);
  const workspace = page.getByRole("navigation", { name: "Module workspace" });

  await openStudioSection(page, "Assessments");
  const assessmentTab = page.getByRole("tablist", { name: "Practice activities" }).getByRole("tab", { name: "Assessments", exact: true });
  await assessmentTab.focus();
  await expect(assessmentTab).toBeFocused();
  await assessmentTab.click();
  const assessment = page.getByTestId("assessment-evidence-panel");
  await expect(assessment.getByRole("heading", { name: "A clearer picture than one score" })).toBeVisible();
  await expect(assessment.getByRole("group", { name: "Assessment purpose" })).toBeVisible();
  await assessment.getByRole("heading", { name: "Compare observations" }).scrollIntoViewIfNeeded();
  await assessment.getByRole("button", { name: "Start", exact: true }).click();
  let assessmentForm = page.getByRole("heading", { name: "Compare observations" }).locator("xpath=ancestor::section[1]");
  const answer = assessmentForm.getByRole("textbox", { name: "Your answer" });
  await answer.click();
  await page.keyboard.type("Record the measure and the observation period.");
  await expect(assessmentForm.getByText("1 of 1 answered")).toBeVisible();
  await assessmentForm.getByRole("button", { name: "Pause attempt", exact: true }).click();
  const pauseDialog = page.getByRole("alertdialog", { name: "Pause this attempt?" });
  await pauseDialog.getByRole("button", { name: "Pause attempt", exact: true }).click();
  await expect(page.getByRole("button", { name: "Start fresh attempt", exact: true })).toBeVisible();
  await assessment.getByRole("button", { name: "Start fresh attempt", exact: true }).click();
  assessmentForm = page.getByRole("heading", { name: "Compare observations" }).locator("xpath=ancestor::section[1]");
  const resumedAnswer = assessmentForm.getByRole("textbox", { name: "Your answer" });
  await resumedAnswer.click();
  await page.keyboard.type("The measure and its observation period.");
  await assessmentForm.getByRole("button", { name: "Submit assessment", exact: true }).click();
  await page.getByRole("alertdialog", { name: "Submit this assessment?" }).getByRole("button", { name: "Submit now", exact: true }).click();
  await expect(page.getByText("Submitted result is not available yet.", { exact: true })).toBeVisible();
  const assessmentState = await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { assessmentWorkspace: { forms: Array<{ status: string; retakeOfFormId: string | null }> }; assessmentForms: Map<string, { items: Array<{ textResponse: string }> }> } }).__LATTICE_LEARNING_STATE__;
    return { statuses: state.assessmentWorkspace.forms.map((form) => form.status), savedResponses: [...state.assessmentForms.values()].map((form) => form.items[0]?.textResponse) };
  });
  expect(assessmentState.statuses).toEqual(["submitted", "interrupted"]);
  expect(assessmentState.savedResponses).toContain("The measure and its observation period.");

  await openStudioSection(page, "Plan");
  const plan = page.getByTestId("learning-plan-panel");
  await expect(plan.getByRole("heading", { name: "Revise with the work in view" })).toBeVisible();
  await expect(plan.getByText("Required lesson count")).toBeVisible();
  await plan.getByRole("button", { name: "Edit", exact: true }).first().click();
  const lessonDialog = page.getByRole("dialog", { name: "Edit lesson" });
  const titleInput = lessonDialog.getByLabel("Title");
  await titleInput.click();
  await titleInput.fill("");
  await page.keyboard.type("Compare field measurements");
  const objectiveInput = lessonDialog.getByLabel("Objective");
  await objectiveInput.click();
  await objectiveInput.fill("");
  await page.keyboard.type("Record a measure and the period used to observe it.");
  await lessonDialog.getByRole("button", { name: "Add to preview", exact: true }).click();
  await plan.getByRole("button", { name: "Preview changes", exact: true }).click();
  await expect(plan.getByRole("heading", { name: "Review before accepting" })).toBeVisible();
  await expect(plan.getByText("Update Compare field measurements.", { exact: true })).toBeVisible();
  await plan.getByRole("button", { name: "Accept revision", exact: true }).click();
  await expect(plan.getByText("Accepted revision 2", { exact: true })).toBeVisible();
  expect(pageErrors.map((error) => error.message)).toEqual([]);
  const curriculum = await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { planWorkspace: { acceptedRevision: { revisionNumber: number; modules: Array<{ lessons: Array<{ title: string; objective: string }> }> }; draftRevision: unknown } } }).__LATTICE_LEARNING_STATE__;
    return { title: state.planWorkspace.acceptedRevision.modules[0].lessons[0].title, objective: state.planWorkspace.acceptedRevision.modules[0].lessons[0].objective, draft: state.planWorkspace.draftRevision };
  });
  expect(curriculum).toEqual({ title: "Compare field measurements", objective: "Record a measure and the period used to observe it.", draft: null });

  await openStudioSection(page, "Lessons");
  await expect(page.getByRole("button", { name: /^Compare field measurements/ })).toBeVisible();

  await openStudioSection(page, "Code & simulations");
  const labs = page.getByTestId("practical-workbench-panel");
  await expect(labs.getByRole("heading", { name: "Work through a real brief" })).toBeVisible();
  await expect(labs.getByRole("heading", { name: "This device cannot run this activity right now" })).toBeVisible();
  await expect(labs.getByText("Docker is not installed in this test environment.", { exact: false }).first()).toBeVisible();
  const runLocally = labs.getByRole("button", { name: "Run locally", exact: true });
  await expect(runLocally).toBeDisabled();
  await expect(labs.getByRole("region", { name: "Run output" })).toHaveCount(0);

  await openStudioSection(page, "Import & export");
  const portability = page.getByTestId("learning-portability-workspace");
  await expect(portability.getByRole("heading", { name: "Keep a safe copy of your program" })).toBeVisible();
  await portability.getByRole("button", { name: "Choose pack and preview" }).click();
  const preview = portability.getByRole("heading", { name: "Imported field observations" });
  await expect(preview).toBeVisible();
  await expect(portability.getByText("Dry run · pending")).toBeVisible();
  const privacy = portability.getByRole("region", { name: "Pack privacy manifest" });
  await expect(privacy).toContainText("Answer keys included");
  await expect(privacy).toContainText("Hidden evaluators included");
  await expect(privacy).toContainText("Learner evidence included");
  await expect(privacy).toContainText("Practical artifacts included");
  await expect(privacy).toContainText("Full source bodies included");
  await expect(privacy).toContainText("Source redistribution confirmed");
  await expect(privacy).toContainText("hosted runtime secrets");
  await expect(portability.getByRole("button", { name: "Apply these changes" })).toBeEnabled();
  await portability.getByRole("button", { name: "Cancel preview" }).click();
  const cancelled = await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { packMutationCalls: Array<{ command: string }>; portabilityWorkspace: { imports: unknown[]; importPreviews: Array<{ status: string }> } } }).__LATTICE_LEARNING_STATE__;
    return { cancelCalls: state.packMutationCalls.filter((item) => item.command === "cancel_learning_pack_import_preview").length, importCount: state.portabilityWorkspace.imports.length, status: state.portabilityWorkspace.importPreviews[0]?.status };
  });
  expect(cancelled).toMatchObject({ cancelCalls: 1, importCount: 0, status: "cancelled" });

  await portability.getByRole("button", { name: "Choose pack and preview" }).click();
  await expect(preview).toBeVisible();
  await portability.getByRole("button", { name: "Apply these changes" }).click();
  const confirm = portability.getByRole("dialog", { name: "Apply the reviewed import?" });
  await expect(confirm).toBeVisible();
  await expect(confirm.getByRole("button", { name: "Confirm import" })).toBeFocused();
  await confirm.getByRole("button", { name: "Confirm import" }).click();
  await expect(portability.getByText("Dry run · applied")).toBeVisible();
  await expect(portability.getByText("Imported program program-copy-e2e.")).toBeVisible();

  await workspace.getByRole("tab", { name: "Sources", exact: true }).click();
  const sources = page.getByTestId("source-maintenance");
  await expect(sources.getByRole("heading", { name: "Reconnect quotes and saved versions" })).toBeVisible();
  await sources.getByRole("button", { name: "Remove source" }).click();
  await sources.getByLabel("Reason for removal").fill("Retaining historical citations");
  await sources.getByRole("button", { name: "Confirm removal" }).click();
  await expect(sources.getByText("Historical tombstone")).toBeVisible();
  await expect(sources.getByText(/Retaining historical citations/)).toBeVisible();
  await sources.getByLabel("Saved version").selectOption("source-version-field-notes-1");
  await sources.getByRole("button", { name: "Re-import selected version" }).click();
  await expect(sources.getByRole("status")).toContainText("was restored as the active source version");

  await workspace.getByRole("tab", { name: "Recall", exact: true }).click();
  const recall = page.getByTestId("learning-recall-evolution");
  await expect(recall.getByRole("heading", { name: "Cards that can evolve" })).toBeVisible();
  await expect(recall.getByText(/scheduler choice is recorded per card/)).toBeVisible();
  await recall.getByRole("button", { name: "Edit card content", exact: true }).first().click();
  const recallPrompt = recall.getByRole("textbox", { name: "Card prompt" });
  await recallPrompt.click();
  await recallPrompt.fill("");
  await page.keyboard.type("Which measure and period should be recorded?");
  await recall.getByRole("textbox", { name: "Why was this card changed?" }).fill("Clarify the comparison details");
  await recall.getByRole("button", { name: "Save card version", exact: true }).click();
  await expect(recall.getByRole("status")).toContainText("Card version saved.");
  await recall.getByRole("button", { name: "Reveal answer", exact: true }).first().click();
  await recall.getByRole("button", { name: "good", exact: true }).first().click();
  await expect(recall.getByRole("status")).toContainText("Review saved to this card’s schedule.");
  await recall.getByRole("button", { name: "Mark as duplicate", exact: true }).click();
  await expect(recall.getByRole("status")).toContainText("Duplicate suggestion confirmed.");
  const recallState = await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { recallV2Workspace: { cards: Array<{ id: string; contentRevision: number; content: { prompt: string }; scheduler: { reviewCount: number }; versions: unknown[]; reviewHistory: unknown[] }>; duplicates: Array<{ status: string }> } } }).__LATTICE_LEARNING_STATE__;
    const card = state.recallV2Workspace.cards.find((item) => item.id === "recall-card-e2e");
    return { revision: card?.contentRevision, prompt: card?.content.prompt, versionCount: card?.versions.length, scheduler: card?.scheduler, reviewCount: card?.reviewHistory.length, duplicate: state.recallV2Workspace.duplicates[0]?.status };
  });
  expect(recallState).toMatchObject({ revision: 2, prompt: "Which measure and period should be recorded?", versionCount: 2, scheduler: { reviewCount: 1 }, reviewCount: 1, duplicate: "confirmed" });

  if (width === 390) {
    const overflow = await page.evaluate(() => Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) - window.innerWidth);
    expect(overflow).toBeLessThanOrEqual(1);
  }
  expect(external.externalRequests).toEqual([]);
  external.stop();
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
  await expectNoUnsupportedIpc(page);
}

test("Learning Studio assessment, curriculum, runtime-unavailable lab, source, recall, and pack workflows on desktop", async ({ page }) => {
  await runExtendedStudioJourney(page, 1440, 1000);
});

test("Learning Studio assessment, curriculum, runtime-unavailable lab, recall, source, and pack workflows fit 390px", async ({ page }) => {
  await runExtendedStudioJourney(page, 390, 844);
});

test("built-in practice remains available without Docker and C# setup becomes ready after refresh", async ({ page }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  await openStudioSection(page, "Code & simulations");
  const environments = page.getByRole("button", { name: /Execution environments/ });
  await expect(environments).toHaveAttribute("aria-expanded", "false");
  await environments.click();
  const setup = page.locator("#learning-runtime-setup");
  await expect(setup.getByText("Included with Lattice", { exact: true }).first()).toBeVisible();
  await expect(setup.getByRole("button", { name: "Set up C#", exact: true })).toBeDisabled();
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { practicalWorkspace: { runtimeCapabilities: Array<{ engine: string; available: boolean; reason: string | null; version: string | null }> } } }).__LATTICE_LEARNING_STATE__;
    state.practicalWorkspace.runtimeCapabilities = [{ engine: "docker", available: true, reason: null, version: "fixture" }];
  });
  await setup.getByRole("button", { name: /Refresh/ }).click();
  const install = setup.getByRole("button", { name: "Set up C#", exact: true });
  await expect(install).toBeEnabled();
  await install.click();
  await expect(setup.getByText("Ready", { exact: true }).last()).toBeVisible();
  await expect.poll(async () => page.evaluate(() => (window as unknown as { __LATTICE_LEARNING_STATE__: { practicalWorkspace: { runtimeProfiles: Array<{ name: string }> } } }).__LATTICE_LEARNING_STATE__.practicalWorkspace.runtimeProfiles.filter((profile) => profile.name === "C#").length)).toBe(1);
  await expectNoUnsupportedIpc(page);
});

async function openLab(page: Page) {
  await openStudioSection(page, "Code & simulations");
  const editor = page.getByRole("textbox", { name: "Edit analysis.txt", exact: true });
  await expect(editor).toHaveAttribute("contenteditable", "true");
  return editor;
}

test("lab drafts survive immediate program navigation and renderer restart without a run", async ({ page }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  const editor = await openLab(page);
  const lines = ["measure = 'mean response time'", "period = 'one quarter'", "print(measure, period)"];
  await editor.fill(lines.join("\n"));
  await page.getByRole("button", { name: "All programs", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Programs", exact: true })).toBeVisible();
  await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
  await expect((await openLab(page)).locator(".cm-line")).toHaveText(lines);
  await page.reload();
  await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
  await expect((await openLab(page)).locator(".cm-line")).toHaveText(lines);
  const saved = await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { labDrafts: Map<string, { draftRevision: number; files: Array<{ content: string }> }>; practicalWorkspace: { runs: unknown[] } } }).__LATTICE_LEARNING_STATE__;
    return { drafts: [...state.labDrafts.values()], runs: state.practicalWorkspace.runs.length };
  });
  expect(saved.drafts[0]).toMatchObject({ draftRevision: 1, files: [{ content: lines.join("\n") }] });
  expect(saved.runs).toBe(0);
  await expectNoUnsupportedIpc(page);
});

test("failed lab saves block leaving and retry lost acknowledgements without duplicating a draft", async ({ page }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  const editor = await openLab(page);
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_LEARNING_STATE__: { labDraftFailuresBeforeSave: number } }).__LATTICE_LEARNING_STATE__.labDraftFailuresBeforeSave = 20;
  });
  await editor.fill("important_work = True");
  await page.getByRole("button", { name: "All programs", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("The draft could not be saved");
  await expect(editor).toContainText("important_work = True");
  await expect(page.getByRole("heading", { name: "Programs", exact: true })).toHaveCount(0);
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_LEARNING_STATE__: { labDraftFailuresBeforeSave: number } }).__LATTICE_LEARNING_STATE__.labDraftFailuresBeforeSave = 0;
  });
  await page.getByRole("button", { name: "Retry saving draft", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved on this device" })).toBeVisible();
  const releaseLostSave = await holdNextLostSaveResponse(page, "lab");
  await editor.fill("important_work = 'second version'");
  await page.getByRole("button", { name: "All programs", exact: true }).click();
  await expect(page.getByRole("button", { name: "Saving your work…", exact: true })).toBeDisabled();
  await releaseLostSave();
  await expect(page.getByRole("alert")).toContainText("Draft save response was lost");
  await page.getByRole("button", { name: "Retry saving draft", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Saved on this device" })).toBeVisible();
  const saved = await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { labDrafts: Map<string, { draftRevision: number }>; labDraftSaveCalls: Array<{ operationId: string }>; labDraftOperations: Map<string, unknown> } }).__LATTICE_LEARNING_STATE__;
    return { revision: [...state.labDrafts.values()][0].draftRevision, lastCalls: state.labDraftSaveCalls.slice(-2), operations: state.labDraftOperations.size };
  });
  expect(saved.revision).toBe(2);
  expect(saved.operations).toBe(2);
  expect(saved.lastCalls[0]).toEqual(saved.lastCalls[1]);
  await expectNoUnsupportedIpc(page);
});

test("activity setup traps focus, supports Escape, and requires a language for runnable labs", async ({ page }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  await openLab(page);
  const opener = page.getByRole("button", { name: "New activity", exact: true });
  await opener.click();
  const dialog = page.getByRole("dialog", { name: "Practice in context" });
  await expect.poll(() => dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);
  await page.keyboard.press("Shift+Tab");
  await expect.poll(() => dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true);
  await dialog.getByLabel("Your focus for this activity").fill("Work through a short example.");
  await expect(dialog.getByRole("button", { name: "Generate activity", exact: true })).toBeDisabled();
  await dialog.getByLabel("Execution environment").selectOption("builtin:python");
  await expect(dialog.getByRole("button", { name: "Generate activity", exact: true })).toBeEnabled();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(opener).toBeFocused();
  await opener.click();
  await dialog.getByRole("button", { name: "Set up C#, Rust, or React & TypeScript", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Execution environments/ })).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByRole("button", { name: /Execution environments/ })).toBeFocused();
  await expectNoUnsupportedIpc(page);
});

test("code editing provides syntax, line numbers, indentation, search, undo, and a keyboard exit", async ({ page }) => {
  await installLearningStudioBackend(page);
  await openProgram(page);
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { practicalWorkspace: { activities: Array<{ files: Array<{ path: string; content: string }> }> } } }).__LATTICE_LEARNING_STATE__;
    state.practicalWorkspace.activities[0].files[0] = { ...state.practicalWorkspace.activities[0].files[0], path: "solution.py", content: "value = 42\nprint(value)" };
  });
  await openStudioSection(page, "Code & simulations");
  const editor = page.getByRole("textbox", { name: "Edit solution.py", exact: true });
  await expect(editor).toHaveAttribute("contenteditable", "true");
  await editor.scrollIntoViewIfNeeded();
  await expect(page.locator(".cm-lineNumbers")).toBeVisible();
  await expect.poll(() => editor.locator(".cm-line span").count()).toBeGreaterThan(0);
  await editor.click();
  await editor.press("ControlOrMeta+Home");
  await editor.press("Tab");
  await expect(editor.locator(".cm-line").first()).toHaveText("  value = 42", { useInnerText: true });
  await editor.press("ControlOrMeta+z");
  await expect(editor.locator(".cm-line").first()).toHaveText("value = 42");
  await page.getByRole("button", { name: /Find in/ }).click();
  const search = page.locator(".cm-search input").first();
  await expect(search).toBeVisible();
  await search.pressSequentially("value");
  await expect.poll(() => page.locator(".cm-searchMatch").count()).toBeGreaterThan(0);
  await search.press("Escape");
  await editor.focus();
  await editor.press("Escape");
  await editor.press("Tab");
  await expect.poll(() => editor.evaluate((element) => element.contains(document.activeElement))).toBe(false);
  await expectNoUnsupportedIpc(page);
});

test("C# files remain readable at 390px and search controls have contrast in both themes", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installLearningStudioBackend(page);
  await openProgram(page);
  await page.evaluate(() => {
    const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { practicalWorkspace: { activities: Array<{ files: Array<{ path: string; content: string }> }> } } }).__LATTICE_LEARNING_STATE__;
    state.practicalWorkspace.activities[0].files[0] = {
      ...state.practicalWorkspace.activities[0].files[0], path: "Program.cs",
      content: "using System;\n\nstatic int Add(int left, int right)\n{\n    return left + right;\n}\n\nConsole.WriteLine(Add(3, 4));",
    };
  });
  await openStudioSection(page, "Code & simulations");
  const editor = page.getByRole("textbox", { name: "Edit Program.cs", exact: true });
  await expect(editor).toHaveAttribute("contenteditable", "true");
  await expect.poll(() => editor.locator(".cm-line span").count()).toBeGreaterThan(0);
  await editor.scrollIntoViewIfNeeded();
  // The app shell clips its own overflow; document width alone misses a broken
  // inner grid. Inspect every editor ancestor through the actual scroll pane.
  const overflows = await editor.evaluate((element) => {
    const result: Array<{ element: string; overflow: number }> = [];
    for (let current: Element | null = element; current; current = current.parentElement) {
      result.push({ element: current.className, overflow: current.scrollWidth - current.clientWidth });
    }
    return result.filter((item) => item.overflow > 1);
  });
  expect(overflows).toEqual([]);
  await page.getByRole("button", { name: /Find in/ }).click();
  const search = page.locator(".cm-search input").first();
  await search.pressSequentially("return");
  for (const theme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    const controls = page.locator(".cm-search button:not([name='close'])");
    await expect(controls.first()).toHaveCSS("background-image", "none");
    const contrast = await controls.evaluateAll(buttonContrast);
    for (const button of contrast) expect(button.ratio, `${theme} ${button.label}`).toBeGreaterThanOrEqual(4.5);
    await page.screenshot({ path: test.info().outputPath(`studio-csharp-search-${theme}-390px.png`), animations: "disabled" });
  }
  await search.press("Escape");
  await page.getByRole("button", { name: "New activity", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Practice in context" });
  await dialog.getByLabel("Execution environment").selectOption("builtin:python");
  await dialog.getByLabel("Your focus for this activity").fill("Work through a short example.");
  for (const theme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    const contrast = await page.locator("button.bg-accent:not(:disabled)").evaluateAll(buttonContrast);
    for (const button of contrast) expect(button.ratio, `${theme} ${button.label}`).toBeGreaterThanOrEqual(4.5);
    await page.screenshot({ path: test.info().outputPath(`studio-activity-dialog-${theme}-390px.png`), animations: "disabled" });
  }
  await expectNoUnsupportedIpc(page);
});


test("topic-only course builder exposes depth, optional Spaces, and a complete syllabus", async ({ page }) => {
  await installLearningStudioBackend(page);
  await page.goto("/studio?new=1");
  await expect(page.getByRole("heading", { name: "What would you like to learn?" })).toBeVisible();
  await page.getByLabel("Your goal", { exact: true }).fill("Learn to design and interpret experiments");
  await page.getByRole("radio", { name: /Deep dive/ }).check();
  await expect(page.getByText("6–10 modules · 24–60 lessons")).toBeVisible();
  await page.getByRole("heading", { name: "What would you like to learn?" }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: "e2e-results/study-course-builder.png", fullPage: true });
  await page.getByRole("button", { name: "Add optional materials" }).click();
  await expect(page.getByLabel("Document Space")).toHaveValue("space_general");
  await expect(page.getByText("No indexed documents in this Space.", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "Create outline", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Program title" })).toHaveValue("Learn to design and interpret experiments");
  await expect(page.getByRole("region", { name: "Course syllabus" })).toBeVisible();
  await expect(page.getByText("Topic-based course · AI-authored", { exact: false })).toBeVisible();
  await expectNoUnsupportedIpc(page);
});


test("topic course creation stays usable at 390px", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installLearningStudioBackend(page);
  await page.goto("/studio?new=1");
  await page.getByLabel("Your goal", { exact: true }).fill("Learn statistics for experiments");
  await page.getByRole("radio", { name: /Complete course/ }).check();
  const overflow = await page.evaluate(() => Math.max(...Array.from(document.querySelectorAll("main, main *")).filter((element) => element.clientWidth > 0 && getComputedStyle(element).overflowX !== "hidden").map((element) => element.scrollWidth - element.clientWidth)));
  expect(overflow).toBeLessThanOrEqual(1);
  await page.getByRole("button", { name: "Create outline", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Program title" })).toHaveValue("Learn statistics for experiments");
  await expect(page.getByRole("region", { name: "Course syllabus" })).toBeVisible();
  await expectNoUnsupportedIpc(page);
});


test("course generation shows real stages and permits cancellation without losing the form", async ({ page, browserName }) => {
  await installLearningStudioBackend(page);
  await page.goto("/studio?new=1");
  await page.getByLabel("Your goal").fill("Learn Rust ownership and build a small command-line tool");
  await page.getByRole("radio", { name: /Deep dive/ }).check();
  await page.getByRole("button", { name: "Add optional materials" }).click();
  await page.getByRole("textbox", { name: /Reference URLs/ }).fill("https://doc.rust-lang.org/book/print.html");
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_OUTLINE_TEST__: { hold: boolean } }).__LATTICE_OUTLINE_TEST__.hold = true;
  });
  await page.getByRole("button", { name: "Create outline", exact: true }).click();
  const panel = page.getByRole("region", { name: "Course generation progress" });
  await expect(panel.getByRole("status")).toHaveText("Reading your references");
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_OUTLINE_TEST__: { update: (value: unknown) => void } }).__LATTICE_OUTLINE_TEST__.update({
      stage: "repairing", elapsedSeconds: 240, stageSeconds: 95, responseCharacters: 12480, modelName: "Progress fixture model",
    });
  });
  await expect(panel.getByRole("status")).toHaveText("Revising the course outline");
  await expect(panel.getByText(/12,480 characters received/)).toBeVisible();
  await panel.scrollIntoViewIfNeeded();
  if (browserName === "chromium") await page.screenshot({ path: "e2e-results/learning-outline-progress-desktop.png" });
  await page.setViewportSize({ width: 390, height: 844 });
  await panel.scrollIntoViewIfNeeded();
  await expect(panel.getByRole("button", { name: "Cancel generation" })).toBeEnabled();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  if (browserName === "chromium") await page.screenshot({ path: "e2e-results/learning-outline-progress-mobile.png" });
  await panel.getByRole("button", { name: "Cancel generation" }).click();
  await expect(page.getByRole("alert")).toContainText("generation cancelled");
  await expect(page.getByLabel("Your goal")).toHaveValue("Learn Rust ownership and build a small command-line tool");
  await expect(page.getByRole("textbox", { name: /Reference URLs/ })).toHaveValue("https://doc.rust-lang.org/book/print.html");
  await expect(page.getByRole("radio", { name: /Deep dive/ })).toBeChecked();
  await expect(page.getByRole("button", { name: "Create outline", exact: true })).toBeEnabled();
  await expectNoUnsupportedIpc(page);
});


test("course generation preserves structured backend errors and retry inputs", async ({ page }) => {
  await installLearningStudioBackend(page);
  await page.goto("/studio?new=1");
  await page.getByLabel("Your goal").fill("Learn Rust and build a command-line tool");
  await page.getByRole("radio", { name: /Complete course/ }).check();
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_OUTLINE_TEST__: { hold: boolean } }).__LATTICE_OUTLINE_TEST__.hold = true;
  });
  await page.getByRole("button", { name: "Create outline", exact: true }).click();
  await expect(page.getByRole("region", { name: "Course generation progress" })).toBeVisible();
  await page.evaluate(() => {
    (window as unknown as { __LATTICE_OUTLINE_TEST__: { fail: (error: unknown) => void } }).__LATTICE_OUTLINE_TEST__.fail({
      code: "TIMEOUT", message: "Course generation timed out while checking the revised outline. Your inputs are kept. Try a focused course or a faster model.",
    });
  });
  await expect(page.getByRole("alert")).toContainText("timed out while checking the revised outline");
  await expect(page.getByRole("alert")).not.toContainText("Service not available");
  await expect(page.getByLabel("Your goal")).toHaveValue("Learn Rust and build a command-line tool");
  await expect(page.getByRole("radio", { name: /Complete course/ })).toBeChecked();
  await expect(page.getByRole("button", { name: "Create outline", exact: true })).toBeEnabled();
  await expectNoUnsupportedIpc(page);
});
