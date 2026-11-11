import { expect, test } from "@playwright/test";

import type {
  CreateInterviewStudyDeckRequestDto, LearningPracticalActivityDto, LearningPracticalWorkspaceDto,
  ReviewStudyCardRequestDto, SetStudyDeckTargetRequestDto, StudyCardDto, StudyDeckDto, StudyDeckSummaryDto,
} from "../src/lib/bindings";
import { mockCommands } from "./fixtures/tauri";
import { installLearningStudioBackend } from "./helpers/learningStudioBackend";
import { openStudioSection } from "./helpers/learningStudioNavigation";

for (const width of [1280, 390]) {
  test(`interview drill: make a deck from an interview, set the date, relearn a confident miss at ${width}px`, async ({ page }, testInfo) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.setViewportSize({ width, height: 900 });
    await installLearningStudioBackend(page);
    await mockCommands(page, () => {
      const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { practicalWorkspace: LearningPracticalWorkspaceDto } }).__LATTICE_LEARNING_STATE__;
      const lessonId = state.practicalWorkspace.activities[0]?.lessonId ?? "lesson";
      const activity: LearningPracticalActivityDto = {
        id: "interview-activity", programId: state.practicalWorkspace.programId, lessonId,
        predecessorId: null, kind: "interview", title: "Acme backend interview", brief: "Explain decisions and tradeoffs.",
        status: "ready", practiceMode: "practice", allowedAids: [], outcomeIds: [], sourceVersionIds: [],
        rubric: [{ id: "reasoning", title: "Reasoning", description: "Explain tradeoffs.", maxPoints: 5 }],
        runtimeKind: "none", runtimeProfileId: null, runtimeEngine: null, runtimeImageId: null,
        runtimeCommand: null, runtimeLimits: null, runtimeAvailable: false, runtimeUnavailableReason: null,
        generatorModel: "Browser fixture", files: [], revision: 0, createdAt: 1, updatedAt: 1,
        interviewPlan: {
          companyName: "Acme", targetRole: "Backend Engineer", roundKind: "full_loop",
          companySummary: "Acme runs streaming pipelines.", learnerRole: "Candidate", counterpartRole: "Acme interviewer",
          researchedSourceVersionIds: [],
          questions: [
            { id: "q-conflict", round: "behavioral", question: "Tell me about a disagreement with a teammate.", companyRationale: "Teams own services end to end.", followUpProbes: ["What would you do differently?"], evaluationSignals: ["Specific situation"] },
            { id: "q-backpressure", round: "technical_deep_dive", question: "How do you handle backpressure in event streams?", companyRationale: "Acme runs streaming pipelines.", followUpProbes: ["What do you drop first?"], evaluationSignals: ["Bounds every queue"] },
          ],
        },
      };
      state.practicalWorkspace.activities = [activity];
      let deck: StudyDeckDto | null = null;
      const summary = (current: StudyDeckDto): StudyDeckSummaryDto => ({
        id: current.id, title: current.title, focus: current.focus, studyGoal: current.studyGoal, createdAt: current.createdAt,
        cardCount: current.cards.length, dueCount: current.cards.filter((card) => card.dueAt <= Date.now()).length,
        quizAttempts: 0, quizCorrect: 0, targetAt: current.targetAt ?? null,
      });
      const source = { chunkId: "guide", documentId: "", fileName: "Answer guide · Acme backend interview", filePath: "", excerpt: "Reference answer." };
      return {
        create_interview_study_deck: (args) => {
          const request = args.request as CreateInterviewStudyDeckRequestDto;
          const now = Date.now();
          const cards: StudyCardDto[] = [
            { id: "card-conflict", format: "question_answer", deckId: "drill", question: "Tell me about a disagreement with a teammate.", answer: "- A specific situation\n- What you did\n- The result and what you learned", options: [], correctIndex: 0, explanation: "**A strong answer**\n\nSituation, task, action, result.", source, citations: [], topic: "Behavioral", dueAt: now, intervalDays: 0, reviewCount: 0, lapses: 0 },
            { id: "card-backpressure", format: "question_answer", deckId: "drill", question: "How do you handle backpressure in event streams?", answer: "- Bounded queues\n- Signal producers to slow down", options: [], correctIndex: 0, explanation: "**Common mistakes**\n\n- Unbounded buffers", source, citations: [], topic: "Technical deep-dive", dueAt: now, intervalDays: 0, reviewCount: 0, lapses: 0 },
          ];
          deck = { id: "drill", title: "Acme · Backend Engineer interview drill", focus: "Interview", studyGoal: "Answer each question aloud from memory, then check it against the guide", modelName: "Browser fixture", createdAt: now, targetAt: request.targetAt, cards };
          return structuredClone(deck);
        },
        list_study_decks: () => (deck ? [summary(deck)] : []),
        get_study_deck: () => {
          if (!deck) throw new Error("Study deck no longer exists");
          return structuredClone(deck);
        },
        set_study_deck_target: (args) => {
          const request = args.request as SetStudyDeckTargetRequestDto;
          if (!deck || deck.id !== request.deckId) throw new Error("Study deck no longer exists");
          deck.targetAt = request.targetAt;
          return structuredClone(deck);
        },
        review_study_card: (args) => {
          const request = args.request as ReviewStudyCardRequestDto;
          const card = deck?.cards.find((item) => item.id === request.cardId);
          if (!card) throw new Error("Study card no longer exists");
          if (card.reviewCount !== request.expectedReviews) throw new Error("This card was already reviewed.");
          card.reviewCount++;
          if (request.rating === "again") card.lapses++;
          card.dueAt = Date.now() + (request.rating === "again" ? 10 : 6 * 60) * 60_000;
          return structuredClone(card);
        },
      };
    }, undefined);

    await page.goto("/studio");
    await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
    await openStudioSection(page, "Code & simulations");
    const drill = page.getByRole("region", { name: /Drill these 2 questions/ });
    await expect(drill).toBeVisible();
    await drill.getByLabel("Interview date and time").fill("2099-01-02T09:30");
    await drill.getByRole("button", { name: "Make drill deck" }).click();
    const created = await page.evaluate(() => window.__LATTICE_IPC__.calls.find((call) => call.command === "create_interview_study_deck")?.args.request);
    expect(created).toMatchObject({ activityId: "interview-activity", targetAt: new Date("2099-01-02T09:30").getTime() });

    await expect(page.getByRole("heading", { name: "Acme · Backend Engineer interview drill" })).toBeVisible();
    await expect(page.getByRole("heading", { name: /Interview or exam/ })).toBeVisible();
    await expect(page.getByText("First pass, now")).toBeVisible();
    await expect(page.getByText("Warm-up")).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("interview-drill-deck.png"), fullPage: true });

    await page.getByRole("button", { name: "Review due (2)" }).click();
    await expect(page.getByRole("heading", { level: 2, name: "Tell me about a disagreement with a teammate." })).toBeVisible();
    await expect(page.getByText(/Answer aloud/)).toBeVisible();
    await page.getByLabel(/Your answer/).fill("We argued about a migration and I proposed a test plan.");
    await page.getByRole("button", { name: "Reveal answer: I know this" }).click();
    await expect(page.getByText("We argued about a migration and I proposed a test plan.")).toBeVisible();
    await page.getByRole("checkbox", { name: "What you did" }).check();
    await expect(page.getByText("You covered 1 of 3: that suggests Again.")).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("interview-drill-reveal.png"), fullPage: true });
    await page.getByRole("button", { name: "Again" }).click();
    await expect(page.getByRole("note")).toContainText("You were sure, and it was wrong.");
    await page.getByRole("button", { name: "Continue" }).click();

    await expect(page.getByRole("heading", { level: 2, name: "How do you handle backpressure in event streams?" })).toBeVisible();
    await expect(page.getByText("2 of 2 · Acme · Backend Engineer interview drill", { exact: true })).toBeVisible();
    await page.keyboard.press("2");
    await page.getByRole("checkbox", { name: "Bounded queues" }).check();
    await page.getByRole("checkbox", { name: "Signal producers to slow down" }).check();
    await page.keyboard.press("3");

    await expect(page.getByText(/missed this earlier/)).toBeVisible();
    await expect(page.getByRole("heading", { level: 2, name: "Tell me about a disagreement with a teammate." })).toBeVisible();
    await page.getByRole("button", { name: "Reveal answer: Not sure" }).click();
    for (const point of ["A specific situation", "What you did", "The result and what you learned"]) await page.getByRole("checkbox", { name: point }).check();
    await page.getByRole("button", { name: "Good" }).click();

    await expect(page.getByRole("heading", { name: "Review complete" })).toBeVisible();
    await expect(page.getByText("2 cards · 1 recalled first try")).toBeVisible();
    await expect(page.getByRole("heading", { name: "You were sure, and it was wrong" })).toBeVisible();
    await expect(page.getByText(/before you sleep/)).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("interview-drill-summary.png"), fullPage: true });

    const reviews = await page.evaluate(() => window.__LATTICE_IPC__.calls.filter((call) => call.command === "review_study_card").map((call) => call.args.request));
    expect(reviews).toMatchObject([
      { cardId: "card-conflict", rating: "again", expectedReviews: 0, selectedOption: null },
      { cardId: "card-backpressure", rating: "good", expectedReviews: 0 },
      { cardId: "card-conflict", rating: "good", expectedReviews: 1 },
    ]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    expect(errors).toEqual([]);
    expect(await page.evaluate(() => window.__LATTICE_IPC__.unsupported)).toEqual([]);
  });
}
