import { expect, test } from "@playwright/test";

import type {
  FinishLearningSimulationRequestDto, GenerateLearningPracticalActivityRequestDto,
  LearningInterviewPlanDto, LearningPracticalActivityDto, LearningPracticalWorkspaceDto,
  LearningSimulationSessionDto, SendLearningSimulationTurnRequestDto, StartLearningSimulationRequestDto,
} from "../src/lib/bindings";
import { mockCommands } from "./fixtures/tauri";
import { installLearningStudioBackend } from "./helpers/learningStudioBackend";
import { openStudioSection } from "./helpers/learningStudioNavigation";

for (const scenario of [{ width: 1280, mode: "practice" }, { width: 390, mode: "practice" }, { width: 390, mode: "demonstrate" }] as const) {
  test(`company interview intake, first question, feedback and repeat attempt at ${scenario.width}px (${scenario.mode})`, async ({ page }, testInfo) => {
    const errors: string[] = [];
    const remoteImages: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => {
      if (request.url().startsWith("https://untrusted-image.example/")) remoteImages.push(request.url());
    });
    await page.setViewportSize({ width: scenario.width, height: 900 });
    await installLearningStudioBackend(page);
    await mockCommands(page, () => {
      const state = (window as unknown as { __LATTICE_LEARNING_STATE__: { practicalWorkspace: LearningPracticalWorkspaceDto } }).__LATTICE_LEARNING_STATE__;
      state.practicalWorkspace.activities = [];
      const plan: LearningInterviewPlanDto = {
        companyName: "Acme", companyUrl: "https://acme.example", targetRole: "Staff Engineer",
        roundKind: "system_design", companySummary: "Company preparation dossier about reliable request processing.",
        learnerRole: "Staff candidate", counterpartRole: "Acme interviewer", researchedSourceVersionIds: [],
        questions: [0, 1, 2].map((index) => ({
          id: `q-${index}`, round: "system_design", question: index === 1
            ? "Assume this practice trace:\n```text\nupload=42\nresponse=lost\nretry=42\n```\nHow would your design prevent duplicate effects?"
            : `How would you design reliable request processing, part ${index + 1}?`,
          companyRationale: "Practice the constraints from the role description.",
          followUpProbes: ["What happens during retries?", "How would you verify it?"],
          evaluationSignals: ["Makes assumptions explicit", "Handles duplicate requests"],
        })),
      };
      plan.questions![2].question = "Explain observability.\n![Untrusted image](https://untrusted-image.example/private-pixel.png)";
      const current = (id: string) => {
        const session = state.practicalWorkspace.simulations.find((item) => item.id === id);
        if (!session) throw new Error("Simulation missing");
        return session;
      };
      return {
        generate_learning_practical_activity: (args) => {
          const request = args.request as GenerateLearningPracticalActivityRequestDto;
          const address = request.interviewTarget?.companyUrl ?? "acme.example";
          plan.companyUrl = address.includes("://") ? address : `https://${address}`;
          const activity: LearningPracticalActivityDto = {
            id: request.activityId, programId: request.programId, lessonId: request.lessonId,
            predecessorId: null, kind: request.kind, title: "Acme staff interview", brief: "Explain decisions and tradeoffs.",
            status: "ready", practiceMode: request.practiceMode, allowedAids: request.allowedAids,
            outcomeIds: [], sourceVersionIds: [], rubric: [{ id: "reasoning", title: "Reasoning", description: "Explain tradeoffs.", maxPoints: 5 }],
            runtimeKind: "none", runtimeProfileId: null, runtimeEngine: null, runtimeImageId: null,
            runtimeCommand: null, runtimeLimits: null, runtimeAvailable: false, runtimeUnavailableReason: null,
            generatorModel: "Browser fixture", files: [], interviewPlan: plan, revision: 0, createdAt: 1, updatedAt: 1,
          };
          state.practicalWorkspace.activities.unshift(activity);
          return structuredClone(state.practicalWorkspace);
        },
        start_learning_simulation: (args) => {
          const request = args.request as StartLearningSimulationRequestDto;
          const session: LearningSimulationSessionDto = {
            id: request.sessionId, programId: request.programId, activityId: request.activityId, activityRevision: 0,
            practiceSessionId: null, learnerRole: request.learnerRole, counterpartRole: request.counterpartRole,
            status: "active", revision: 0, createdAt: 1, updatedAt: 1, submittedAt: null,
            turns: [{ id: crypto.randomUUID(), ordinal: 0, speaker: "counterpart", content: plan.questions![0].question, citations: [], modelName: "Browser fixture", createdAt: 1 }],
          };
          state.practicalWorkspace.simulations.unshift(session);
          return structuredClone(session);
        },
        send_learning_simulation_turn: (args) => {
          const request = args.request as SendLearningSimulationTurnRequestDto;
          const session = current(request.sessionId);
          session.turns.push(
            { id: crypto.randomUUID(), ordinal: session.turns.length, speaker: "learner", content: request.content, citations: [], modelName: null, createdAt: 2 },
            { id: crypto.randomUUID(), ordinal: session.turns.length + 1, speaker: "counterpart", content: "How would you verify behavior under retries?", citations: [], modelName: "Browser fixture", createdAt: 2 },
          );
          session.revision++;
          return structuredClone(session);
        },
        finish_learning_simulation: (args) => {
          const request = args.request as FinishLearningSimulationRequestDto;
          const session = current(request.sessionId);
          session.status = "submitted";
          session.revision++;
          session.turns.push({ id: crypto.randomUUID(), ordinal: session.turns.length, speaker: "coach", content: "Reasoning: 4/5. Next, practice verifying duplicate request behavior.", citations: [], modelName: "Browser fixture", createdAt: 3 });
          return structuredClone(session);
        },
      };
    }, undefined);
    await page.goto("/studio");
    await page.getByRole("button", { name: /Reasoning from field observations/ }).click();
    await openStudioSection(page, "Code & simulations");
    await page.getByRole("button", { name: "New activity", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Practice in context" });
    await dialog.getByLabel("Activity type").selectOption("interview");
    await dialog.getByLabel("Company name", { exact: true }).fill("Acme");
    const companyWebsite = scenario.width === 390 ? `https://acme.example/careers?context=${"reference".repeat(30)}` : "acme.example";
    await dialog.getByLabel("Company website or URL").fill(companyWebsite);
    await dialog.getByLabel("Target role").fill("Staff Engineer");
    await dialog.getByLabel("Interview round").selectOption("system_design");
    await dialog.getByLabel("Job description or company notes").fill("Design reliable request processing with idempotent APIs.");
    if (scenario.mode === "demonstrate") await dialog.getByRole("button", { name: "demonstrate", exact: true }).click();
    await dialog.getByRole("button", { name: "Generate activity", exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "Acme staff interview" })).toBeVisible();
    await expect(page.getByLabel("Your role")).toHaveValue("Staff candidate");
    if (scenario.mode === "demonstrate") {
      await expect(page.getByText("Company preparation dossier about reliable request processing.")).toHaveCount(0);
      await expect(page.getByText("How would you design reliable request processing, part 1?", { exact: true })).toHaveCount(0);
    } else {
      await expect(page.getByText("Company preparation dossier about reliable request processing.")).toBeVisible();
      await expect(page.getByLabel("Company interview dossier").locator("pre").filter({ hasText: "response=lost" })).toBeVisible();
      await expect(page.getByLabel("Company interview dossier").locator('img[src^="https://untrusted-image.example/"]')).toHaveCount(0);
    }
    const generatedRequest = await page.evaluate(() => window.__LATTICE_IPC__.calls.find((call) => call.command === "generate_learning_practical_activity")?.args.request);
    expect(generatedRequest).toMatchObject({
      kind: "interview", practiceMode: scenario.mode,
      interviewTarget: { companyName: "Acme", companyUrl: companyWebsite, targetRole: "Staff Engineer", roundKind: "system_design" },
      runtimeProfileId: null, builtinRuntime: null,
    });
    await page.getByLabel("Company interview dossier").scrollIntoViewIfNeeded();
    expect(await page.getByLabel("Company interview dossier").evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath("company-interview.png"), fullPage: true });
    await page.getByRole("button", { name: "Start simulation", exact: true }).click();
    const turns = page.getByRole("log", { name: "Simulation turns" });
    await expect(turns.getByText("How would you design reliable request processing, part 1?", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Finish simulation" })).toBeDisabled();
    await page.getByLabel("Your response").fill("I would store an idempotency key and test concurrent retries.");
    await page.getByRole("button", { name: "Send simulation turn" }).click();
    await expect(turns.getByText("How would you verify behavior under retries?", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Finish simulation", exact: true }).click();
    await expect(turns.getByText(/Reasoning: 4\/5/)).toBeVisible();
    await openStudioSection(page, "Lessons");
    await openStudioSection(page, "Code & simulations");
    await expect(page.getByText(/Reasoning: 4\/5/)).toBeVisible();
    await page.getByRole("button", { name: "Start another attempt" }).click();
    await expect(page.getByRole("button", { name: "Start simulation", exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    expect(errors).toEqual([]);
    expect(remoteImages).toEqual([]);
    expect(await page.evaluate(() => window.__LATTICE_IPC__.unsupported)).toEqual([]);
  });
}
