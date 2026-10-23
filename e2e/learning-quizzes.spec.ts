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

  await page.getByLabel("Measure", { exact: true }).selectOption({ label: "What is counted" });
  await page.getByLabel("Period", { exact: true }).selectOption({ label: "How long you watched" });
  await page.getByLabel("Limit", { exact: true }).selectOption({ label: "Tomorrow's forecast" });
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
  await expect(row.getByText("Passed · 100 %", { exact: true })).toBeVisible();
  await expect(row.getByText(/latest 100 %|2 attempts/)).toBeVisible();
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
