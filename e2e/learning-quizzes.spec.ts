import { expect, test, type Page } from "@playwright/test";

import { installLearningStudioBackend } from "./helpers/learningStudioBackend";

/**
 * The main quiz flow (quizzes design §2.5): start from the lesson card, answer
 * six formats by keyboard, submit, see keyed results at once and the written
 * answer graded a moment later with its reasons, add misses to Recall, retake
 * with fresh questions, pass, and mark the lesson complete by hand.
 */

async function openCourse(page: Page) {
  await installLearningStudioBackend(page);
  await page.goto("/studio");
  await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
  await expect(page.getByRole("navigation", { name: "Module workspace" })).toBeVisible();
}

async function unsupported(page: Page) {
  return page.evaluate(() => (window as unknown as { __LATTICE_LEARNING_STATE__: { unsupportedCommands: string[] } }).__LATTICE_LEARNING_STATE__.unsupportedCommands);
}

const next = (page: Page) => page.getByRole("button", { name: "Next", exact: true }).click();

/** The first form: one wrong true/false, one wrong pair, a written answer with no example. */
async function answerFirstForm(page: Page) {
  const choice = page.getByRole("radio", { name: /The same observation period/ });
  await choice.focus();
  await page.keyboard.press("Space");
  await expect(choice).toHaveAttribute("aria-checked", "true");
  await next(page);

  await page.getByRole("radio", { name: /True/ }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("radio", { name: /False/ })).toHaveAttribute("aria-checked", "true");
  await next(page);

  for (const [term, answer] of [["Measure", "What is counted"], ["Period", "How long you watched"], ["Limit", "Tomorrow's forecast"]]) {
    await page.getByRole("button", { name: term, exact: true }).focus();
    await page.keyboard.press("Enter");
    await page.getByRole("button", { name: answer, exact: true }).focus();
    await page.keyboard.press("Space");
  }
  await next(page);

  // Ordering without dragging: Alt+↓ twice on the first row.
  await page.getByRole("listitem", { name: "1 of 3: Compare the counts" }).focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page.getByRole("listitem", { name: "2 of 3: Compare the counts" })).toBeFocused();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page.getByRole("status").filter({ hasText: "Compare the counts moved to position 3 of 3." })).toBeAttached();
  await next(page);

  await page.getByRole("textbox", { name: "Blank 1 of 2" }).fill("measure");
  await page.getByRole("textbox", { name: "Blank 2 of 2" }).fill("Period");
  await next(page);

  await page.getByLabel("Your explanation").fill("One sample shows one place and one time only.");
  await expect(page.getByText("6 of 6 answered", { exact: true })).toBeVisible();
}

async function submit(page: Page) {
  await page.getByRole("button", { name: "Submit answers" }).click();
  const dialog = page.getByRole("alertdialog", { name: "Submit your answers?" });
  await expect(dialog.getByText(/Your responses will be frozen and graded/)).toBeVisible();
  await dialog.getByRole("button", { name: "Submit now" }).click();
}

test('quiz cards support visual pairing and pointer sorting in light and dark', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await openCourse(page);
  await page.getByRole('region', { name: 'Check what stuck from “Compare observations”' }).getByRole('button', { name: /Start quiz/ }).click();
  await next(page);
  await next(page);
  await page.getByRole('button', { name: 'What is counted', exact: true }).dragTo(page.getByRole('button', { name: 'Measure', exact: true }));
  await expect(page.getByRole('button', { name: 'Measure', exact: true })).toHaveAttribute('aria-description', 'Matched to What is counted');
  await page.getByRole('button', { name: 'Period', exact: true }).click();
  await page.getByRole('button', { name: 'How long you watched', exact: true }).click();
  await page.getByRole('button', { name: 'Limit', exact: true }).click();
  await page.getByRole('button', { name: 'What the sample cannot show', exact: true }).click();
  await expect(page.getByText('3 / 3 paired')).toBeVisible();
  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: 1000 });
      await expect(page.locator('.ui-pairing-lines path')).toHaveCount(3);
      await page.locator('.ui-pairing-board').scrollIntoViewIfNeeded();
      expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
      await page.screenshot({ path: test.info().outputPath(`quiz-pairing-${theme}-${width}px.png`), animations: 'disabled' });
    }
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
  await next(page);
  const firstHandle = page.getByRole('button', { name: 'Drag Compare the counts', exact: true });
  await firstHandle.hover();
  const cancelTarget = await page.getByRole('listitem', { name: '2 of 3: Choose the measure' }).boundingBox();
  expect(cancelTarget).not.toBeNull();
  await page.mouse.down();
  try {
    await page.mouse.move(cancelTarget!.x + 24, cancelTarget!.y + cancelTarget!.height, { steps: 10 });
    await expect(page.locator('.ui-sortable-ghost')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('.ui-sortable-ghost')).toHaveCount(0);
  } finally {
    await page.mouse.up();
  }
  await expect(page.getByRole('listitem', { name: '1 of 3: Compare the counts' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Question 4, not answered yet', exact: true })).toBeVisible();
  // Each gesture crosses one neighbor. Hover waits for the preceding layout
  // transition before the next grab, keeping the test independent of frame rate.
  for (const [position, neighbor] of [[2, 'Choose the measure'], [3, 'Fix the observation period']] as const) {
    const handle = page.getByRole('button', { name: 'Drag Compare the counts', exact: true });
    await handle.hover();
    const start = await handle.boundingBox();
    const target = await page.getByRole('listitem', { name: `${position} of 3: ${neighbor}` }).boundingBox();
    expect(start).not.toBeNull();
    expect(target).not.toBeNull();
    await page.mouse.down();
    try {
      await page.mouse.move(start!.x + start!.width / 2, target!.y + target!.height, { steps: 16 });
      await expect(page.locator('.ui-sortable-row[data-drop-target="true"]')).toBeVisible();
      // The preview must not save an intermediate answer while the pointer is held.
      await expect(page.getByRole('listitem', { name: `${position - 1} of 3: Compare the counts` })).toBeVisible();
      if (position === 2) await page.screenshot({ path: test.info().outputPath('quiz-sorting-preview-dark.png'), animations: 'disabled' });
    } finally {
      await page.mouse.up();
    }
    await expect(page.getByRole('listitem', { name: `${position} of 3: Compare the counts` })).toBeVisible();
  }
  await expect(page.getByRole('status').filter({ hasText: 'Order updated:' })).toBeAttached();
  await page.screenshot({ path: test.info().outputPath('quiz-sorted-dark.png'), animations: 'disabled' });
  expect(await unsupported(page)).toEqual([]);
});

test("take a lesson quiz, learn why points were lost, retake with fresh questions and pass", async ({ page }) => {
  await openCourse(page);
  const callout = page.getByRole("region", { name: "Check what stuck from “Compare observations”" });
  await expect(callout.getByText("Not taken", { exact: true })).toBeVisible();
  await expect(callout.getByText("Best: not taken yet")).toBeVisible();
  await callout.getByRole("button", { name: /Start quiz/ }).click();

  await expect(page.getByRole("heading", { name: "Check what stuck", exact: true })).toBeVisible();
  await expect(page.getByText("Keys are shown after you submit. You can pause and resume any time.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Submit answers" })).toBeDisabled();
  await answerFirstForm(page);
  await submit(page);

  // Keyed answers are graded at once; the written one is pending, never a fail.
  const verdict = page.locator("#quiz-result-heading");
  await expect(verdict).toHaveText(/Result pending/);
  await expect(verdict).toBeFocused();
  await expect(page.getByText("Grading…", { exact: true })).toBeVisible();
  await expect(page.getByText(/Not passed/)).toHaveCount(0);
  await expect(page.getByText("You chose B; the key is A. It can: the period alone changes the count.")).toBeVisible();
  await expect(page.getByText("2 of 3 pairs matched").first()).toBeVisible();

  // The grading job finishes: a verdict, and the reasons for every lost point.
  await expect(verdict).toHaveText(/Not passed yet/, { timeout: 10_000 });
  const why = page.locator("details").filter({ hasText: "Why this lost points" });
  await expect(why).toHaveAttribute("open", "");
  await expect(why.getByText(/gave no concrete example/)).toBeVisible();
  await expect(why.locator('mark[data-quote="1"]')).toContainText("One sample shows one place and");
  await expect(why.getByText("Rests on quote 1, in your words.")).toBeVisible();
  await expect(why.getByText("Names a second place or time where the count would differ.")).toBeVisible();
  await expect(why.getByText(/^Graded by Configured local model · provisional/)).toBeVisible();

  await page.getByRole("button", { name: "Add 3 missed questions to Recall" }).click();
  await expect(page.getByText("Added 3 cards to Recall. You can edit them there.")).toBeVisible();

  // A retake draws questions the learner has not seen.
  await page.getByRole("button", { name: "Retake with fresh questions" }).click();
  await expect(page.getByText("Which belong in a careful comparison record?")).toBeVisible();
  await expect(page.getByText(/Seen before/)).toHaveCount(0);
  for (const option of [/The measure/, /The observation period/, /The sample's limits/]) {
    await page.getByRole("checkbox", { name: option }).focus();
    await page.keyboard.press("Space");
  }
  await next(page);
  await page.getByLabel("Your answer").fill("Observation period");
  await next(page);
  await page.getByLabel("Your explanation").fill("Watching each pond for the same hour removes time as a cause; a second pond watched longer would show more.");
  await submit(page);
  await expect(verdict).toHaveText(/Passed/, { timeout: 10_000 });
  await expect(page.getByText(/Attempt 2 · /)).toBeVisible();

  // Passing never completes the lesson by itself.
  const complete = page.getByRole("button", { name: "Mark lesson complete" });
  await expect(complete).toBeVisible();
  await complete.click();
  await expect(page.getByText("Lesson marked complete")).toBeVisible();

  await page.getByRole("button", { name: "Quizzes", exact: true }).click();
  const row = page.locator("#quiz-lesson-lesson-observation");
  await expect(row.getByText("Passed · 100 %", { exact: true })).toBeVisible();
  await expect(row.getByText(/latest 100 %|2 attempts/)).toBeVisible();
  expect(await unsupported(page)).toEqual([]);
});

test("pause and resume a quiz, contest a written grade, and override an upheld regrade", async ({ page }) => {
  await openCourse(page);
  await page.getByRole("tablist", { name: "Program workspace" }).getByRole("tab", { name: "Quizzes", exact: true }).click();
  const row = page.locator("#quiz-lesson-lesson-observation");
  await row.getByRole("button", { name: /Start quiz/ }).click();

  // Pause marks the attempt paused; the next answer resumes it.
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await page.getByRole("alertdialog", { name: "Pause this quiz?" }).getByRole("button", { name: "Pause", exact: true }).click();
  await expect(row.getByText("Paused · Resume", { exact: true })).toBeVisible();
  await row.getByRole("button", { name: "Resume", exact: true }).click();
  await expect(page.getByText(/Paused · answer any question to pick up where you left off/)).toBeVisible();
  await answerFirstForm(page);
  await expect(page.getByText(/Paused · answer any question/)).toHaveCount(0);
  await submit(page);
  const why = page.locator("details").filter({ hasText: "Why this lost points" });
  await expect(why).toBeVisible({ timeout: 10_000 });

  await why.getByRole("button", { name: "I think I was right" }).click();
  await why.getByLabel(/What did the grade miss/).fill("The second time of day is my example.");
  await why.getByRole("button", { name: "Ask for a regrade" }).click();
  const regrade = page.getByRole("region", { name: "Your regrade request" });
  await expect(regrade.getByText("Regrade upheld the grade")).toBeVisible();
  await expect(regrade.getByText(/the question asks for one to be named/)).toBeVisible();
  await expect(regrade.getByRole("region", { name: "First grade" })).toBeVisible();
  await expect(regrade.getByRole("region", { name: "Regrade" })).toBeVisible();

  await regrade.getByRole("button", { name: "Override: count it as right" }).click();
  await expect(regrade.getByText(/never counts as mastery evidence/)).toBeVisible();
  await regrade.getByRole("button", { name: "Override", exact: true }).click();
  await expect(page.getByText(/Score includes your override \(1 answer overridden by you\)/)).toBeVisible();
  await expect(page.getByText("Overridden by you").first()).toBeVisible();
  expect(await unsupported(page)).toEqual([]);
});
