import type { Page } from "@playwright/test";

import type {
  LearningAssessmentFormDto,
  LearningAssessmentFormItemDto,
  LearningAssessmentItemResultDto,
  LearningGenerationJob,
  LearningGradeRationaleDto,
  LearningKeyRevealDto,
  LearningPartResult,
  LearningQuizAttemptSummaryDto,
  LearningQuizOverviewDto,
  LearningQuizScopeDto,
  LearningQuizSettingsDto,
  LearningQuizStatusDto,
} from "../../src/lib/bindings";

/**
 * The quiz side of the Learning Studio fixture (quizzes design §6.3): banks
 * per scope, forms with private keys, keyed grading with partial credit, a
 * written answer the "grading job" marks a moment later with a checked
 * rationale, contests, overrides, reports and misses into Recall. Like the
 * claim review backend it runs inside the page, so it closes over nothing.
 */
export function quizBackend() {
  type Key =
    | { kind: "choice"; indices: number[] }
    | { kind: "text"; variants: string[] }
    | { kind: "cloze"; blanks: string[] }
    | { kind: "matching"; matches: number[] }
    | { kind: "ordering"; order: string[] }
    | { kind: "rubric"; strong: boolean };
  type Authored = { item: Partial<LearningAssessmentFormItemDto> & { id: string; format: LearningAssessmentFormItemDto["format"]; prompt: string }; key: Key; explanation: string; rationales?: string[] };
  type Context = { programId: string; lessonId: string; moduleId: string; lessonTitle: string; moduleTitle: string };

  const normalise = (value: string) => value.normalize("NFKD").replace(/[̀-ͯ]/g, "").toLowerCase()
    .replace(/[^\p{L}\p{N}\s.]/gu, " ").replace(/\s+/g, " ").trim().replace(/^(a|an|the) /, "");
  const scopeKey = (scope: LearningQuizScopeDto) => `${scope.kind}:${scope.id ?? "course"}`;

  // Two forms' worth for the lesson: the first attempt draws set A, a retake set B ("fresh questions").
  const setA: Authored[] = [
    { item: { id: "q-choice", format: "single_choice", prompt: "Which detail makes two counts comparable?", options: ["The same observation period", "A larger notebook", "A different observer", "A warmer day"] },
      key: { kind: "choice", indices: [0] }, explanation: "Counts are comparable only over the same period of watching.",
      rationales: ["", "The notebook does not change what was observed.", "A different observer adds a new source of difference.", "The weather is not the measure."] },
    { item: { id: "q-tf", format: "true_false", prompt: "A longer observation period can explain a higher count on its own.", options: ["True", "False"] },
      key: { kind: "choice", indices: [0] }, explanation: "More time watching means more chances to count, with no real difference.", rationales: ["", "It can: the period alone changes the count."] },
    { item: { id: "q-match", format: "matching", prompt: "Match each part of the record with what it says.", leftItems: ["Measure", "Period", "Limit"], rightItems: ["What is counted", "How long you watched", "What the sample cannot show", "Tomorrow's forecast"] },
      key: { kind: "matching", matches: [0, 1, 2] }, explanation: "A careful record names the measure, the period and the limits." },
    { item: { id: "q-order", format: "ordering", prompt: "Put the steps of a fair comparison in order.", options: ["Compare the counts", "Choose the measure", "Fix the observation period"] },
      key: { kind: "ordering", order: ["Choose the measure", "Fix the observation period", "Compare the counts"] }, explanation: "Decide what and how long before comparing." },
    { item: { id: "q-cloze", format: "cloze", prompt: "Record the {{1}} and the observation {{2}} before you compare.", blankCount: 2 },
      key: { kind: "cloze", blanks: ["measure", "period"] }, explanation: "Both are needed to read a count." },
    { item: { id: "q-explain", format: "explanation", prompt: "Explain why one sample cannot support a universal claim.", points: 4,
      rubric: [{ id: "accuracy", title: "Accuracy", description: "States what one sample shows.", maxPoints: 2 }, { id: "example", title: "Example", description: "Gives a concrete case.", maxPoints: 2 }] },
      key: { kind: "rubric", strong: false }, explanation: "A strong answer says one sample shows one place and time, and gives a case where another sample differs." },
  ];
  const setB: Authored[] = [
    { item: { id: "r-multi", format: "multi_select", prompt: "Which belong in a careful comparison record?", options: ["The measure", "The observation period", "A guess about the cause", "The sample's limits"] },
      key: { kind: "choice", indices: [0, 1, 3] }, explanation: "A guess about cause is not part of the record.", rationales: ["", "", "A cause is an inference, not an observation.", ""] },
    { item: { id: "r-short", format: "short_answer", prompt: "What do you call the length of time you watched?" },
      key: { kind: "text", variants: ["observation period", "period"] }, explanation: "The observation period." },
    { item: { id: "r-explain", format: "explanation", prompt: "Explain how a fixed observation period makes counts comparable.", points: 4,
      rubric: [{ id: "accuracy", title: "Accuracy", description: "States the role of the period.", maxPoints: 2 }, { id: "example", title: "Example", description: "Gives a concrete case.", maxPoints: 2 }] },
      key: { kind: "rubric", strong: true }, explanation: "A strong answer says equal time removes time as a cause, with a case." },
  ];

  return function create(context: Context) {
    const state = {
      settings: { quizPass: 0.7, testPass: 0.75, examPass: 0.75, writtenAnswers: true } as LearningQuizSettingsDto,
      banks: new Map<string, "not_prepared" | "preparing" | "ready">([[`lesson:${context.lessonId}`, "ready"]]),
      forms: new Map<string, LearningAssessmentFormDto>(),
      keys: new Map<string, Authored>(),
      drawn: new Map<string, number>(),
      gradeAt: new Map<string, number>(),
      reported: [] as string[],
      recall: [] as string[],
      overrides: [] as string[],
      contests: [] as Array<{ itemId: string; note: string }>,
      calls: [] as string[],
    };
    const request = <T>(args: unknown) => (args as { request: T }).request;
    // The open form: active or paused, the one touched last.
    const openForm = (forms: LearningAssessmentFormDto[]) => {
      const open = forms.filter((form) => form.status === "active" || form.status === "interrupted").sort((a, b) => b.updatedAt - a.updatedAt)[0];
      return { activeFormId: open?.id ?? null, activeFormStartedAt: open?.createdAt ?? null, activeFormUpdatedAt: open?.updatedAt ?? null, activeFormPaused: open?.status === "interrupted" };
    };
    const statusOf = (scope: LearningQuizScopeDto): LearningQuizStatusDto => {
      const key = scopeKey(scope);
      const forms = [...state.forms.values()].filter((form) => form.scope && scopeKey(form.scope) === key && form.mode === "graded");
      const done = forms.filter((form) => form.status === "submitted" && form.submission);
      const chronological = [...done].sort((a, b) => (a.submittedAt ?? 0) - (b.submittedAt ?? 0));
      const summary = (form: LearningAssessmentFormDto): LearningQuizAttemptSummaryDto => ({
        attemptNumber: chronological.indexOf(form) + 1,
        formId: form.id, score: form.submission!.score, passed: form.submission!.passed, gradeStatus: form.submission!.gradeStatus,
        submittedAt: form.submittedAt ?? 0, retakeOfFormId: form.retakeOfFormId, pendingItems: form.submission!.pendingItems,
        overriddenItems: form.submission!.itemResults.filter((result) => result.learnerOverride).length,
      });
      const latest = done.sort((a, b) => (b.submittedAt ?? 0) - (a.submittedAt ?? 0))[0];
      const graded = done.filter((form) => form.submission!.passed !== null);
      const best = [...graded].sort((a, b) => b.submission!.score - a.submission!.score)[0];
      const bank = state.banks.get(key) ?? "not_prepared";
      const drawn = state.drawn.get(key) ?? 0;
      const pass = scope.kind === "lesson" ? state.settings.quizPass : scope.kind === "module" ? state.settings.testPass : state.settings.examPass;
      return {
        scope, title: scope.kind === "lesson" ? context.lessonTitle : scope.kind === "module" ? "Module test" : "Course exam",
        bankStatus: bank, bankMessage: "", bankSize: bank === "ready" ? 18 : 0, freshItems: drawn === 0 ? 6 : drawn === 1 ? 3 : 0, formSize: drawn === 0 ? 6 : 3,
        expectedMinutes: 12, passingScore: pass, coverage: scope.kind === "lesson" ? null : { coveredLessons: 1, totalLessons: 1 },
        ...openForm(forms),
        verdict: latest?.submission!.passed === null ? "pending" : best ? (best.submission!.passed ? "passed" : "not_passed") : "not_taken",
        best: best ? summary(best) : latest ? summary(latest) : null, latest: latest ? summary(latest) : null, attempts: done.length, preparingJobId: null,
      };
    };
    const overview = (): LearningQuizOverviewDto => {
      const lessonQuiz = statusOf({ kind: "lesson", id: context.lessonId });
      return {
        programId: context.programId, settings: state.settings, exam: statusOf({ kind: "course", id: null }),
        modules: [{ moduleId: context.moduleId, test: statusOf({ kind: "module", id: context.moduleId }), lessons: [{ lessonId: context.lessonId, quiz: lessonQuiz }] }],
        quizzesPassed: lessonQuiz.verdict === "passed" ? 1 : 0, quizzesTotal: 1, outcomes: [], evidence: [], followUps: [],
      };
    };
    const blankItem = (authored: Authored, scope: LearningQuizScopeDto, repeat: boolean): LearningAssessmentFormItemDto => ({
      outcomeIds: ["outcome-1"], difficulty: 2, options: [], artifactKind: null, rubric: [], points: 1, sourceVersionIds: [], selectedIndices: [], blankResponses: [],
      matches: (authored.item.leftItems ?? []).map(() => null), leftItems: [], rightItems: [], blankCount: 0, textResponse: null, orderedValues: [], artifactJson: null,
      responseRevision: 0, keyReveal: null, sourceQuote: "", answerHint: authored.key.kind === "text" ? { numeric: false, unitHint: null } : null, ...authored.item, id: `${scopeKey(scope)}:${authored.item.id}`, previouslyExposed: repeat,
    } as LearningAssessmentFormItemDto);
    const reveal = (authored: Authored, item: LearningAssessmentFormItemDto): LearningKeyRevealDto => ({
      correctIndices: authored.key.kind === "choice" ? authored.key.indices : [], acceptedAnswer: authored.key.kind === "text" ? authored.key.variants[0] : null,
      orderedValues: authored.key.kind === "ordering" ? authored.key.order : [], matches: authored.key.kind === "matching" ? authored.key.matches : [],
      blanks: authored.key.kind === "cloze" ? authored.key.blanks : [], explanation: authored.explanation,
      distractorRationales: authored.rationales ?? item.options.map(() => ""),
    });
    type Keyed = { earned: number; detail: string; parts?: LearningPartResult[] };
    const part = (index: number, correct: boolean, why: string): LearningPartResult => ({ index, correct, detail: correct ? "" : why });
    const keyed = (authored: Authored, item: LearningAssessmentFormItemDto): Keyed => {
      const key = authored.key;
      if (key.kind === "choice" && item.format === "multi_select") {
        const hits = item.selectedIndices.filter((index) => key.indices.includes(index)).length;
        const wrong = item.selectedIndices.length - hits;
        const parts = item.options.map((_, index) => part(index, item.selectedIndices.includes(index) === key.indices.includes(index),
          key.indices.includes(index) ? "This belongs in the record." : (authored.rationales?.[index] ?? "")));
        return { earned: Math.max(0, hits - wrong) / key.indices.length, detail: `${hits} of ${key.indices.length} right${wrong ? `, ${wrong} wrong ${wrong === 1 ? "pick" : "picks"}` : ""}`, parts };
      }
      if (key.kind === "choice") return { earned: item.selectedIndices[0] === key.indices[0] ? 1 : 0, detail: "" };
      if (key.kind === "text") {
        const given = (item.textResponse ?? "").trim();
        if (!given) return { earned: 0, detail: "No answer given" };
        return key.variants.some((variant) => normalise(variant) === normalise(given)) ? { earned: 1, detail: "" } : { earned: 0, detail: `"${given}" isn't an accepted answer.` };
      }
      if (key.kind === "cloze") {
        const right = key.blanks.filter((blank, index) => normalise(blank) === normalise(item.blankResponses[index] ?? "")).length;
        const parts = key.blanks.map((blank, index) => part(index, normalise(blank) === normalise(item.blankResponses[index] ?? ""), `The record needs the ${blank} here.`));
        return { earned: right / key.blanks.length, detail: `${right} of ${key.blanks.length} blanks right`, parts };
      }
      if (key.kind === "matching") {
        const right = key.matches.filter((match, index) => item.matches[index] === match).length;
        const parts = key.matches.map((match, index) => part(index, item.matches[index] === match, `${item.leftItems[index]} goes with "${item.rightItems[match]}".`));
        return { earned: right / key.matches.length, detail: `${right} of ${key.matches.length} pairs matched`, parts };
      }
      if (key.kind === "ordering") {
        const order = item.orderedValues.length ? item.orderedValues : item.options;
        let pairs = 0; let right = 0;
        for (let i = 0; i < key.order.length; i += 1) for (let j = i + 1; j < key.order.length; j += 1) {
          pairs += 1;
          if (order.indexOf(key.order[i]) < order.indexOf(key.order[j])) right += 1;
        }
        const parts = order.map((value, index) => part(index, key.order[index] === value, `${value} comes at step ${key.order.indexOf(value) + 1}.`));
        return { earned: right / pairs, detail: `${right} of ${pairs} pairs in order`, parts };
      }
      return { earned: 0, detail: "" };
    };
    const rationaleFor = (authored: Authored, answer: string): LearningGradeRationaleDto => {
      const strong = authored.key.kind === "rubric" && authored.key.strong;
      const words = answer.trim().split(/\s+/).slice(0, 6).join(" ");
      return {
        summary: strong ? "You said what a fixed period removes and gave a case. Full credit." : "You explained what one sample shows, but gave no concrete example, which the question asked for.",
        criteria: [
          { criterionId: "accuracy", title: "Accuracy", verdict: "met", points: 2, maxPoints: 2, whatWasMissing: "", yourWords: words ? [words] : [], fullCreditIncludes: "", sourceQuote: null, sourceVersionId: null },
          strong
            ? { criterionId: "example", title: "Example", verdict: "met", points: 2, maxPoints: 2, whatWasMissing: "", yourWords: [], fullCreditIncludes: "", sourceQuote: null, sourceVersionId: null }
            : { criterionId: "example", title: "Example", verdict: "missed", points: 0, maxPoints: 2, whatWasMissing: "No example of a second sample that differs.", yourWords: [],
              fullCreditIncludes: "Names a second place or time where the count would differ.", sourceQuote: "the limits of the sample", sourceVersionId: "source-version-field-notes-1" },
        ],
        graderModel: "Configured local model", rubricVersion: "rubric-1", gradedAt: Date.now(),
      };
    };
    const score = (form: LearningAssessmentFormDto) => {
      const submission = form.submission!;
      const earned = submission.itemResults.reduce((sum, result) => sum + result.pointsEarned, 0);
      const possible = submission.itemResults.reduce((sum, result) => sum + result.pointsPossible, 0);
      submission.score = possible ? earned / possible : 0;
      submission.pendingItems = submission.itemResults.filter((result) => result.gradeStatus === "pending").length;
      submission.passed = submission.pendingItems ? null : submission.score >= form.passingScore;
      submission.gradeStatus = submission.pendingItems ? "pending" : submission.itemResults.some((result) => result.gradeStatus === "provisional") ? "provisional" : "deterministic";
      submission.gradingJobId = submission.pendingItems ? `quiz-grading-${form.id}` : null;
      submission.graderModel = submission.itemResults.some((result) => result.rationale) ? "Configured local model" : null;
    };
    // The grading job: a written answer is graded a moment after submission, on the next read.
    const finishGrading = (form: LearningAssessmentFormDto) => {
      const at = state.gradeAt.get(form.id);
      if (!form.submission || !form.submission.pendingItems || at === undefined || Date.now() < at) return;
      for (const result of form.submission.itemResults) {
        if (result.gradeStatus !== "pending") continue;
        const authored = state.keys.get(result.itemId)!;
        const item = form.items.find((entry) => entry.id === result.itemId)!;
        const rationale = rationaleFor(authored, item.textResponse ?? "");
        const earned = rationale.criteria.reduce((sum, criterion) => sum + (criterion.points ?? 0), 0);
        Object.assign(result, { gradeStatus: "provisional", pointsEarned: earned, score: earned / result.pointsPossible, correct: earned >= result.pointsPossible, rationale });
      }
      score(form);
    };
    const formOf = (args: unknown) => {
      const id = (args as { id?: string }).id ?? request<{ formId: string }>(args).formId;
      const form = state.forms.get(id);
      if (!form) throw new Error("This quiz could not be found.");
      return form;
    };

    const handlers = {
      get_learning_quiz_overview: () => overview(),
      update_learning_quiz_settings: (args: unknown) => { state.settings = request<{ settings: LearningQuizSettingsDto }>(args).settings; return overview(); },
      prepare_learning_quiz_bank: (args: unknown) => {
        const { scope, operationId } = request<{ scope: LearningQuizScopeDto; operationId: string }>(args);
        state.banks.set(scopeKey(scope), "ready");
        return { id: `quiz-bank-${scopeKey(scope)}`, programId: context.programId, operationId, kind: "quiz_bank", payloadSha256: "fixture", baseRevisionNumber: 0, status: "completed",
          progressCompleted: 3, progressTotal: 3, progressMessage: "Questions ready", activity: null, resultId: null, error: null, retryOfJobId: null, createdAt: Date.now(), startedAt: Date.now(), finishedAt: Date.now() } as LearningGenerationJob;
      },
      start_learning_quiz: (args: unknown) => {
        const { formId, scope, mode, retakeOfFormId } = request<{ formId: string; scope: LearningQuizScopeDto; mode: "graded" | "practice"; retakeOfFormId: string | null }>(args);
        const existing = state.forms.get(formId);
        if (existing) return existing;
        if ((state.banks.get(scopeKey(scope)) ?? "not_prepared") !== "ready") throw new Error("Questions for this quiz are not prepared yet.");
        const drawn = state.drawn.get(scopeKey(scope)) ?? 0;
        const set = mode === "graded" && drawn > 0 ? setB : setA;
        if (mode === "graded") state.drawn.set(scopeKey(scope), drawn + 1);
        const items = set.map((authored) => {
          const item = blankItem(authored, scope, drawn > 1);
          state.keys.set(item.id, authored);
          return item;
        });
        const form: LearningAssessmentFormDto = {
          id: formId, programId: context.programId, blueprintId: `blueprint-${scopeKey(scope)}`, blueprintRevision: 1, retakeOfFormId, status: "active", revision: 0,
          title: scope.kind === "lesson" ? context.lessonTitle : scope.kind === "module" ? context.moduleTitle : "Course exam", instructions: "", expectedMinutes: 12,
          purpose: scope.kind === "lesson" ? "checkpoint" : scope.kind === "module" ? "module_test" : "cumulative",
          passingScore: scope.kind === "lesson" ? state.settings.quizPass : state.settings.testPass, feedbackTiming: "after_submission", sourceVersionIds: [],
          modelName: null, items, createdAt: Date.now(), updatedAt: Date.now(), submittedAt: null, submission: null, scope, mode,
        } as LearningAssessmentFormDto;
        state.forms.set(formId, form);
        return form;
      },
      get_learning_assessment_form: (args: unknown) => { const form = formOf(args); finishGrading(form); return form; },
      save_learning_assessment_response: (args: unknown) => {
        const { expectedRevision, response } = request<{ expectedRevision: number; response: Record<string, unknown> }>(args);
        const form = formOf(args);
        if (form.status === "submitted") throw new Error("This quiz is already submitted.");
        if (form.revision !== expectedRevision) throw new Error("This quiz changed; reload and retry.");
        const item = form.items.find((entry) => entry.id === response.itemId);
        if (!item) throw new Error("Question not found.");
        Object.assign(item, {
          selectedIndices: response.selectedIndices ?? [], textResponse: response.text ?? null, blankResponses: response.blankResponses ?? [],
          matches: (response.matches as unknown[] | undefined)?.length ? response.matches : item.matches, orderedValues: response.orderedValues ?? [],
          responseRevision: item.responseRevision + 1,
        });
        if (form.mode === "practice") item.keyReveal = reveal(state.keys.get(item.id)!, item);
        // Answering a paused quiz resumes it.
        form.status = "active";
        form.revision += 1;
        form.updatedAt = Date.now();
        return form;
      },
      interrupt_learning_assessment_form: (args: unknown) => {
        const form = formOf(args);
        if (form.status !== "active") throw new Error("Only an open quiz can be paused.");
        form.status = "interrupted"; form.revision += 1; form.updatedAt = Date.now();
        return form;
      },
      submit_learning_assessment_form: (args: unknown) => {
        const form = formOf(args);
        if (form.status === "submitted") return form;
        form.status = "submitted";
        form.submittedAt = Date.now();
        form.revision += 1;
        const itemResults: LearningAssessmentItemResultDto[] = form.items.map((item) => {
          const authored = state.keys.get(item.id)!;
          item.keyReveal = reveal(authored, item);
          const base = { itemId: item.id, outcomeIds: item.outcomeIds, feedback: "", learnerOverride: false, rationale: null, contest: null, partResults: [] as LearningPartResult[], pointsPossible: item.points };
          if (item.format === "explanation") return { ...base, score: 0, correct: null, gradeStatus: "pending", pointsEarned: 0, detail: "" } as LearningAssessmentItemResultDto;
          const { earned, detail, parts } = keyed(authored, item);
          return { ...base, score: earned, correct: earned >= 1, gradeStatus: "deterministic", pointsEarned: earned * item.points, detail, partResults: parts ?? [] } as LearningAssessmentItemResultDto;
        });
        form.submission = { score: 0, passed: null, gradeStatus: "deterministic", graderModel: null, graderDisagreement: [], feedback: "", itemResults, submittedAt: form.submittedAt,
          scoreWithoutOverrides: null, pendingItems: 0, gradingJobId: null } as LearningAssessmentFormDto["submission"];
        score(form);
        state.gradeAt.set(form.id, Date.now() + 1500);
        return form;
      },
      resume_learning_quiz_grading: (args: unknown) => { const form = formOf(args); state.gradeAt.set(form.id, Date.now() + 500); score(form); return form; },
      contest_learning_quiz_grade: (args: unknown) => {
        const { itemId, note } = request<{ itemId: string; note: string }>(args);
        const form = formOf(args);
        const result = form.submission!.itemResults.find((entry) => entry.itemId === itemId)!;
        state.contests.push({ itemId, note });
        result.contest = { id: `contest-${itemId}`, status: "upheld", note, firstScore: result.pointsEarned, regradeScore: result.pointsEarned, regradeReason: "The answer still names no second sample.",
          firstRationale: result.rationale!, regradeRationale: result.rationale!, responseToNote: "Your note says the example was implied; the question asks for one to be named.", createdAt: Date.now(), decidedAt: Date.now() };
        return form;
      },
      override_learning_quiz_grade: (args: unknown) => {
        const { itemId } = request<{ itemId: string }>(args);
        const form = formOf(args);
        const result = form.submission!.itemResults.find((entry) => entry.itemId === itemId)!;
        if (result.contest?.status !== "upheld") throw new Error("An override follows an upheld regrade.");
        const before = form.submission!.score;
        Object.assign(result, { learnerOverride: true, pointsEarned: result.pointsPossible, contest: { ...result.contest, status: "overridden" } });
        state.overrides.push(itemId);
        score(form);
        form.submission!.scoreWithoutOverrides = before;
        return form;
      },
      report_learning_quiz_item: (args: unknown) => { state.reported.push(request<{ itemId: string }>(args).itemId); return formOf(args); },
      add_learning_quiz_misses_to_recall: (args: unknown) => {
        const { itemIds } = request<{ itemIds: string[] }>(args);
        state.recall.push(...itemIds);
        return { cardsCreated: itemIds.length, deckId: "deck-learning-1" };
      },
      // The screens read the overview again after a decision; answer with the evidence fields both shapes share.
      accept_learning_follow_up: () => ({ programId: context.programId, outcomes: [], evidence: [], followUps: [] }),
      dismiss_learning_follow_up: () => ({ programId: context.programId, outcomes: [], evidence: [], followUps: [] }),
    };
    return { state, handlers };
  };
}

export type QuizBackend = ReturnType<typeof quizBackend>;

/** Puts `quizBackend()` on the page as `window.__LATTICE_QUIZ_BACKEND__` for the Learning Studio mock. */
export async function installQuizBackend(page: Page) {
  await page.addInitScript({ content: `window.__LATTICE_QUIZ_BACKEND__ = (${quizBackend.toString()})();` });
}
