import type { Page } from "@playwright/test";

import type {
  LearningMemoryDto,
  LearningOutlineProgressDto,
  LearningGenerationJob,
  LearningGenerationJobActionRequestDto,
  LearningLessonDto,
  LearningProgramDto,
  LearningPracticalWorkspaceDto,
  StudyCardDto,
  StudyDeckSummaryDto,
  WorkspaceNoteDto,
} from "../../src/lib/bindings";
import { makeAppSettings } from "../../src/tests/fixtures/appSettings";

/** Collect any HTTP(S) requests the renderer sends outside the local app. */
export function watchExternalHttpRequests(page: Page, localOrigin: string) {
  const externalRequests: string[] = [];
  const handler = (request: { url: () => string }) => {
    try {
      const url = new URL(request.url());
      if (
        (url.protocol === "http:" || url.protocol === "https:") &&
        url.origin !== localOrigin
      ) {
        externalRequests.push(url.href);
      }
    } catch {
      // Non-HTTP resource URLs such as data: and blob: are intentionally local.
    }
  };
  page.on("request", handler);
  return {
    externalRequests,
    stop: () => page.off("request", handler),
  };
}

type SourceKind = "web" | "document" | "pasted";
type SourcePolicy = "fixed" | "manual" | "before_use";
type SourceCheckStatus = "unchanged" | "update_available" | "failed";
type SourceVersionSummary = {
  id: string;
  versionNumber: number;
  title: string;
  publisher: string | null;
  resolvedUrl: string | null;
  excerpt: string;
  contentSha256: string;
  wordCount: number;
  truncated: boolean;
  extractionVersion: string;
  acquiredAt: number;
};
type SourceCheck = {
  operationId: string;
  status: SourceCheckStatus;
  checkedAt: number;
  activeDigest: string | null;
  pendingVersionId: string | null;
  message: string | null;
};
type SourceLibraryItem = {
  id: string;
  kind: SourceKind;
  origin: string;
  requestedUrl: string | null;
  freshnessPolicy: SourcePolicy;
  activeVersionId: string | null;
  pendingVersionId: string | null;
  revision: number;
  activeVersion: SourceVersionSummary | null;
  pendingVersion: SourceVersionSummary | null;
  versions: SourceVersionSummary[];
  latestCheck: SourceCheck | null;
  checks: SourceCheck[];
  createdAt: number;
  updatedAt: number;
  deletedAt?: number | null;
  deletionReason?: string | null;
};
type SourceWorkspace = { programId: string; sources: SourceLibraryItem[] };
type SourceVersionDetail = {
  sourceId: string;
  version: SourceVersionSummary;
  fullText: string;
  usage: Array<{
    lessonId: string;
    lessonTitle: string;
    referenceKind: string;
    referenceTitle: string;
  }>;
};

type PracticeMode = "explore" | "practice" | "demonstrate";
type PracticeHintLevel =
  | "orienting_question"
  | "concept_or_source"
  | "partial_strategy"
  | "worked_explanation";
type PracticeCitation = {
  sourceId: string;
  versionId: string;
  quote: string;
};
type PracticeCriterion = {
  id: string;
  dimension: "recall" | "explanation" | "application" | "transfer";
  title: string;
  description: string;
  maxPoints: number;
};
type PracticeAssistanceEvent = {
  id: string;
  kind:
    | "source_opened"
    | "hint"
    | "tutor_response"
    | "solution_revealed"
    | "mode_changed";
  mode: PracticeMode;
  artifactRevision: number;
  details: Record<string, unknown>;
  createdAt: number;
};
type PracticeProposal = {
  id: string;
  kind: "misconception" | "follow_up";
  text: string;
  evidenceQuote: string | null;
  tutorTurnId: string | null;
  status: "pending" | "accepted" | "rejected";
  createdAt: number;
  decidedAt: number | null;
};
type PracticeSession = {
  summary: {
    id: string;
    lessonId: string;
    lessonTitle: string;
    status: "active" | "submitted";
    mode: PracticeMode;
    revision: number;
    artifactRevision: number;
    sourceVersionIds: string[];
    createdAt: number;
    updatedAt: number;
    submittedAt: number | null;
    gradeStatus: "provisional" | "uncertain" | null;
    taskKind?: "guided" | "independent";
    revisesSessionId?: string | null;
  };
  taskPrompt: string;
  lessonObjective: string;
  rubric: PracticeCriterion[];
  artifact: { revision: number; text: string; sha256: string; updatedAt: number };
  assistance: PracticeAssistanceEvent[];
  tutorTurns: Array<{
    id: string;
    prompt: string;
    requestKind: "hint" | "question" | "critique";
    response: string;
    hintLevel: PracticeHintLevel | null;
    citations: PracticeCitation[];
    proposalIds: string[];
    modelName: string;
    createdAt: number;
  }>;
  proposals: PracticeProposal[];
  revealedSolution: string | null;
  revealedSolutionCitations: PracticeCitation[];
  result: {
    gradeStatus: "provisional" | "uncertain";
    artifactRevision: number;
    artifactText: string;
    rubric: PracticeCriterion[];
    criteria: Array<{
      criterionId: string;
      dimension: PracticeCriterion["dimension"];
      score: number | null;
      maxPoints: number;
      observation: string;
      evidenceQuote: string | null;
    }>;
    evidence: Array<{
      dimension: PracticeCriterion["dimension"];
      observed: boolean;
      observation: string;
      evidenceQuote: string | null;
      assistanceKinds: PracticeAssistanceEvent["kind"][];
    }>;
    modeAtSubmission: PracticeMode;
    assistance: PracticeAssistanceEvent[];
    graderModel: string;
    submittedAt: number;
  } | null;
};
type PracticeWorkspace = {
  programId: string;
  sessions: Array<PracticeSession["summary"]>;
};

type LabDraft = {
  programId: string;
  activityId: string;
  activityRevision: number;
  draftRevision: number;
  files: Array<{ path: string; content: string }>;
  updatedAt: number | null;
};

/** Install a stateful Tauri command fixture for Learning Studio route tests. */
export async function installLearningStudioBackend(page: Page) {
  await page.addInitScript((settings) => {
    Object.defineProperty(window, "isTauri", {
      configurable: true,
      value: true,
    });

    const source = {
      id: "source-version-field-notes-1",
      title: "Public field notes",
      url: "https://example.org/field-notes",
      excerpt:
        "SOURCE_EXCERPT_SENTINEL: A careful comparison records the chosen measure, the observation period, and the limits of the sample.",
      acquiredAt: 1_790_000_000_000,
    };
    const lesson: LearningLessonDto = {
      id: "lesson-observation",
      title: "Compare observations",
      objective: "Explain a comparison and note its limits.",
      estimatedMinutes: 25,
      preparation: "ready",
      completed: false,
      blocks: [
        {
          kind: "explanation",
          title: "Choose a measure",
          body: "A comparison begins by naming the measure and the period under observation.",
          sourceIds: [source.id],
        },
        {
          kind: "worked_example",
          title: "Read a sample",
          body: "Record the chosen measure and observation period, then check which cases are absent.",
          sourceIds: [source.id],
        },
        {
          kind: "guided_practice",
          title: "Try a comparison",
          body: "Compare observations recorded over different periods. State the first change you would make before interpreting the difference, and explain why.",
          sourceIds: [source.id],
        },
        {
          kind: "independent_practice",
          title: "Design your comparison",
          body: "Create a measurement plan with a consistent observation period, explain the comparison, and identify one limitation.",
          sourceIds: [source.id],
        },
        {
          kind: "reflection",
          title: "State a limit",
          body: "Describe one inference the sample does not support.",
          sourceIds: [source.id],
        },
      ],
      questions: [],
    };
    const program: LearningProgramDto = {
      summary: {
        id: "program-learning-1",
        title: "Reasoning from field observations",
        goal: "Practice careful comparison using a saved field excerpt.",
        status: "active",
        revision: 2,
        moduleCount: 1,
        lessonCount: 1,
        completedLessons: 0,
        currentLessonId: lesson.id,
        createdAt: 1_790_000_000_000,
      },
      priorKnowledge: "",
      minutesPerSession: 25,
      modelName: "Configured local model",
      sources: [source],
      modules: [
        {
          id: "module-observation",
          title: "Comparing observations",
          summary: "A small, source-grounded course.",
          outcomes: ["Name a measure", "Describe a limitation"],
          lessons: [lesson],
        },
      ],
      attempts: [],
    };
    const memory: LearningMemoryDto = {
      programId: program.summary.id,
      journalId: "journal-learning-1",
      lessonNotes: [],
      studyDeck: null,
      drafts: [],
      acceptedCards: [],
      dueCount: 0,
      schedulerVersion: "expanding_v1",
    };
    type CanvasSnapshot = {
      id: string;
      canvasId: string;
      name: string;
      title: string;
      description: string;
      sceneJson: Record<string, unknown>;
      elementCount: number;
      canvasRevision: number;
      createdAt: number;
    };
    type Canvas = {
      id: string;
      programId: string;
      lessonId: string | null;
      title: string;
      description: string;
      sceneJson: Record<string, unknown>;
      elementCount: number;
      revision: number;
      createdAt: number;
      updatedAt: number;
      snapshots: CanvasSnapshot[];
    };
    const canvasWorkspace: { programId: string; canvases: Canvas[] } = {
      programId: program.summary.id,
      canvases: [],
    };
    const canvasOperations = new Map<
      string,
      { command: string; payload: string }
    >();
    const sourceV1: SourceVersionSummary = {
      id: "source-version-field-notes-1",
      versionNumber: 1,
      title: "Public field notes",
      publisher: "Field Notes Archive",
      resolvedUrl: "https://example.org/field-notes",
      excerpt:
        "ACTIVE_SOURCE_SENTINEL: Record the same measure across the observation period.",
      contentSha256: "a".repeat(64),
      wordCount: 18,
      truncated: false,
      extractionVersion: "bounded_text_v1",
      acquiredAt: 1_790_000_000_000,
    };
    const sourceWorkspace: SourceWorkspace = {
      programId: program.summary.id,
      sources: [
        {
          id: "logical-source-field-notes",
          kind: "web",
          origin: source.url ?? "https://example.org/field-notes",
          requestedUrl: source.url,
          freshnessPolicy: "manual",
          activeVersionId: sourceV1.id,
          pendingVersionId: null,
          revision: 1,
          activeVersion: sourceV1,
          pendingVersion: null,
          versions: [sourceV1],
          latestCheck: null,
          checks: [],
          createdAt: sourceV1.acquiredAt,
          updatedAt: sourceV1.acquiredAt,
        },
      ],
    };
    const sourceVersionTexts = new Map<string, string>([
      [
        sourceV1.id,
        "ACTIVE_SOURCE_SENTINEL: Record the same measure across the observation period. The original edition remains available for exact citation review.",
      ],
    ]);
    const sourceOperations = new Map<
      string,
      { command: string; payload: string }
    >();
    const practiceWorkspace: PracticeWorkspace = {
      programId: program.summary.id,
      sessions: [],
    };
    const practiceSessions = new Map<string, PracticeSession>();
    const practiceOperations = new Map<
      string,
      { command: string; payload: string }
    >();
    const practiceRubric: PracticeCriterion[] = [
      {
        id: "recall",
        dimension: "recall",
        title: "Identify the measure",
        description: "Name the measure used in the comparison.",
        maxPoints: 3,
      },
      {
        id: "explanation",
        dimension: "explanation",
        title: "Explain the comparison",
        description: "Connect the measure to the observation period.",
        maxPoints: 3,
      },
      {
        id: "application",
        dimension: "application",
        title: "Apply the method",
        description: "Use the method in a specific example.",
        maxPoints: 3,
      },
      {
        id: "transfer",
        dimension: "transfer",
        title: "State a limitation",
        description: "Identify what the sample cannot establish.",
        maxPoints: 3,
      },
    ];
    const state = {
      program,
      memory,
      canvasWorkspace,
      calls: [] as { command: string; args: unknown }[],
      unsupportedCommands: [] as string[],
      // The app retries mutations once automatically before surfacing its
      // recoverable error, so two transport failures reach the explicit retry UI.
      generationFailuresRemaining: 1,
      // Mutations surface the failure; the next user attempt reuses the review ID.
      reviewFailuresRemaining: 1,
      reviewRequests: [] as Array<{
        reviewId: string;
        cardId: string;
        expectedReviews: number;
        selectedOption: number | null;
        rating: string;
      }>,
      appliedReviewIds: {} as Record<string, unknown>,
      noteWrites: 0,
      canvasMutationCalls: [] as Array<{
        command: string;
        request: Record<string, unknown>;
      }>,
      canvasSaveLostResponsesRemaining: 0,
      lostSaveResponseGate: null as Promise<void> | null,
      releaseLostSaveResponse: null as (() => void) | null,
      canvasSavedRevisions: [] as number[],
      sourceWorkspace,
      sourceMutationCalls: [] as Array<{
        command: string;
        request: Record<string, unknown>;
      }>,
      sourceLostResponsesRemaining: 0,
      nextSourceRefreshOutcome: "unchanged" as
        "unchanged" | "changed" | "failed",
      sourceAppliedEffects: {} as Record<string, number>,
      practiceWorkspace,
      practiceMutationCalls: [] as Array<{
        command: string;
        request: Record<string, unknown>;
      }>,
      practiceAppliedEffects: {} as Record<string, number>,
      practiceLostResponsesRemaining: 0,
      practiceSessions,
      assessmentWorkspace: {
        programId: program.summary.id,
        outcomes: [],
        blueprints: [],
        forms: [],
        evidence: [],
        followUps: [],
      },
      assessmentForms: new Map<string, Record<string, unknown>>(),
      planWorkspace: {
        programId: program.summary.id,
        programRevision: program.summary.revision,
        acceptedRevision: null,
        draftRevision: null,
        previewChanges: [],
        requiredLessonCountBefore: lesson ? 1 : 0,
        requiredLessonCountAfter: lesson ? 1 : 0,
        resumeLessonId: lesson?.id ?? null,
        jobs: [] as LearningGenerationJob[],
        latestDiagnostic: null as Record<string, unknown> | null,
      },
      practicalWorkspace: {
        programId: program.summary.id,
        builtinRuntimes: [
          { id: "python", name: "Python", description: "Python standard-library exercises. Included with Lattice.", available: true, reason: null },
          { id: "javascript", name: "JavaScript", description: "JavaScript module exercises. Included with Lattice.", available: true, reason: null },
        ],
        runtimeCapabilities: [],
        runtimeProfiles: [],
        activities: [],
        runs: [],
        simulations: [],
      } as LearningPracticalWorkspaceDto,
      labDrafts: new Map<string, LabDraft>(),
      labDraftOperations: new Map<string, { payload: string; result: LabDraft }>(),
      labDraftSaveCalls: [] as Array<Record<string, unknown>>,
      labDraftFailuresBeforeSave: 0,
      labDraftLostResponses: 0,
      portabilityWorkspace: {
        programId: program.summary.id,
        exports: [],
        importPreviews: [],
        imports: [],
        sourceWorkspace,
      } as { programId: string; exports: unknown[]; importPreviews: Array<{ id: string; status: string; manifest: { rootSha256: string }; canApply: boolean; decidedAt: number | null; changes: unknown[] }>; imports: unknown[]; sourceWorkspace: SourceWorkspace },
      recallV2Workspace: {
        programId: program.summary.id,
        cards: [],
        duplicates: [],
        dueCount: 0,
        fsrsAvailable: false,
        schedulerDisclosure: "The FSRS schedule is not available in this test runtime.",
      },
      packMutationCalls: [] as Array<{ command: string; request: Record<string, unknown> }>,
    };
    const assessmentBlueprint = {
      id: "assessment-blueprint-e2e",
      programId: program.summary.id,
      purpose: "practice",
      revision: 1,
      title: "Compare observations",
      instructions: "Use the saved source to explain a careful comparison.",
      requirements: [{ format: "short_answer", count: 1 }],
      status: "accepted",
      createdAt: 1_790_000_000_000,
      acceptedAt: 1_790_000_000_001,
    };
    Object.assign(state.assessmentWorkspace, { blueprints: [assessmentBlueprint] });
    const curriculumLesson = {
      id: lesson.id,
      title: lesson.title,
      objective: lesson.objective,
      estimatedMinutes: lesson.estimatedMinutes,
      state: "outline",
      assessmentStarted: false,
      replacementLessonId: null,
    };
    const acceptedRevision = {
      id: "curriculum-revision-e2e-1",
      programId: program.summary.id,
      revisionNumber: 1,
      status: "accepted",
      modules: [{ id: "module-e2e-1", title: "Field methods", purpose: "Compare observations carefully.", outcomeIds: ["outcome-1"], prerequisiteModuleIds: [], lessons: [curriculumLesson] }],
      createdAt: 1_790_000_000_000,
      acceptedAt: 1_790_000_000_001,
    };
    Object.assign(state.planWorkspace, { acceptedRevision });
    state.planWorkspace.requiredLessonCountBefore = 1;
    state.planWorkspace.requiredLessonCountAfter = 1;
    const practicalActivity = {
      id: "practical-unavailable-e2e",
      programId: program.summary.id,
      lessonId: lesson.id,
      kind: "code_lab",
      title: "Compare field measurements",
      brief: "Record one measure across a defined observation period.",
      generatorModel: "Deterministic fixture",
      status: "ready",
      revision: 1,
      runtimeKind: "container",
      runtimeProfileId: "profile-docker-e2e",
      runtimeAvailable: false,
      runtimeUnavailableReason: "Docker is not installed in this test environment.",
      practiceMode: "practice",
      allowedAids: ["Saved sources"],
      rubric: [{ id: "measure", title: "Name the measure", description: "Identify the measure in the comparison." }],
      files: [{ path: "analysis.txt", role: "starter", editable: true, content: "measure = 'observation'\nperiod = 'one week'\n" }],
      checks: [],
      createdAt: 1_790_000_000_000,
      updatedAt: 1_790_000_000_000,
    };
    Object.assign(state.practicalWorkspace, {
      runtimeCapabilities: [{ engine: "docker", available: false, version: null, reason: "Docker is not installed in this test environment." }],
      runtimeProfiles: [{ id: "profile-docker-e2e", name: "Local Docker", engine: "docker", enabled: true, command: ["python", "analysis.txt"], imageId: "sha256:fixture", limits: { timeoutSeconds: 10, memoryMegabytes: 256, cpuMillis: 500, processLimit: 32, outputBytes: 8192 }, revision: 1, createdAt: 1_790_000_000_000, updatedAt: 1_790_000_000_000 }],
      activities: [practicalActivity],
    });
    const recallCard = {
      id: "recall-card-e2e",
      programId: program.summary.id,
      format: "question_answer",
      content: { prompt: "Which details define a careful comparison?", answer: "The measure and observation period.", explanation: "Both make the comparison interpretable.", options: [], correctOptionIndex: null, language: null, clozeDeletions: [] },
      contentRevision: 1,
      versions: [{ revision: 1, format: "question_answer", content: { prompt: "Which details define a careful comparison?", answer: "The measure and observation period.", explanation: "Both make the comparison interpretable.", options: [], correctOptionIndex: null, language: null, clozeDeletions: [] }, sourceVersionIds: [sourceV1.id], changeReason: "Initial source-backed card", createdAt: 1_790_000_000_000 }],
      sourceVersionIds: [sourceV1.id],
      scheduler: { schedulerVersion: "expanding_v1", stability: null, difficulty: null, lastReviewedAt: null, dueAt: 0, intervalDays: 0, reviewCount: 0 },
      reviewHistory: [],
      createdAt: 1_790_000_000_000,
      updatedAt: 1_790_000_000_000,
    };
    Object.assign(state.recallV2Workspace, {
      cards: [recallCard, { ...JSON.parse(JSON.stringify(recallCard)), id: "recall-card-match-e2e", content: { ...recallCard.content, prompt: "What should a careful comparison record?" }, versions: [{ ...recallCard.versions[0], content: { ...recallCard.content, prompt: "What should a careful comparison record?" } }], sourceVersionIds: [sourceV1.id] }],
      duplicates: [{ id: "duplicate-e2e", cardId: recallCard.id, possibleDuplicateCardId: "recall-card-match-e2e", reason: "Saved prompts cover closely related comparison details.", similarity: 0.91, status: "pending", createdAt: 1_790_000_000_001, decidedAt: null }],
      dueCount: 1,
      fsrsAvailable: true,
      schedulerDisclosure: "The scheduler choice is recorded per card in this deterministic fixture.",
    });
    const persistedPracticeStateKey = "__learning_studio_practice_backend__";
    const labDraftStorageKey = "__learning_studio_lab_drafts__";
    try {
      const saved = JSON.parse(sessionStorage.getItem(labDraftStorageKey) ?? "null") as {
        drafts: [string, LabDraft][];
        operations: [string, { payload: string; result: LabDraft }][];
      } | null;
      if (saved) {
        state.labDrafts = new Map(saved.drafts);
        state.labDraftOperations = new Map(saved.operations);
      }
    } catch {
      // Malformed fixtures never become permissive command handlers.
    }
    const persistLabDrafts = () => sessionStorage.setItem(labDraftStorageKey, JSON.stringify({
      drafts: [...state.labDrafts], operations: [...state.labDraftOperations],
    }));
    try {
      const persisted = JSON.parse(
        sessionStorage.getItem(persistedPracticeStateKey) ?? "null",
      ) as {
        programId?: string;
        workspace?: PracticeWorkspace;
        sessions?: [string, PracticeSession][];
        operations?: [string, { command: string; payload: string }][];
        mutationCalls?: typeof state.practiceMutationCalls;
        appliedEffects?: Record<string, number>;
      } | null;
      if (persisted?.programId === program.summary.id) {
        practiceWorkspace.sessions = persisted.workspace?.sessions ?? [];
        for (const [id, session] of persisted.sessions ?? [])
          practiceSessions.set(id, session);
        for (const [id, operation] of persisted.operations ?? [])
          practiceOperations.set(id, operation);
        state.practiceMutationCalls = persisted.mutationCalls ?? [];
        state.practiceAppliedEffects = persisted.appliedEffects ?? {};
      }
    } catch {
      // Ignore malformed test storage without weakening the command fixture.
    }
    const persistPracticeState = () => {
      sessionStorage.setItem(
        persistedPracticeStateKey,
        JSON.stringify({
          programId: program.summary.id,
          workspace: practiceWorkspace,
          sessions: [...practiceSessions.entries()],
          operations: [...practiceOperations.entries()],
          mutationCalls: state.practiceMutationCalls,
          appliedEffects: state.practiceAppliedEffects,
        }),
      );
    };
    const persistedCanvasStateKey = "__learning_studio_canvas_backend__";
    try {
      const persisted = JSON.parse(
        sessionStorage.getItem(persistedCanvasStateKey) ?? "null",
      ) as {
        programId?: string;
        canvases?: Canvas[];
        operations?: [string, { command: string; payload: string }][];
        mutationCalls?: typeof state.canvasMutationCalls;
        savedRevisions?: number[];
      } | null;
      if (persisted?.programId === program.summary.id) {
        canvasWorkspace.canvases = persisted.canvases ?? [];
        for (const [key, value] of persisted.operations ?? [])
          canvasOperations.set(key, value);
        state.canvasMutationCalls = persisted.mutationCalls ?? [];
        state.canvasSavedRevisions = persisted.savedRevisions ?? [];
      }
    } catch {
      // Keep a malformed prior mock snapshot from making the command mock permissive.
    }
    const persistCanvasState = () => {
      sessionStorage.setItem(
        persistedCanvasStateKey,
        JSON.stringify({
          programId: program.summary.id,
          canvases: canvasWorkspace.canvases,
          operations: [...canvasOperations.entries()],
          mutationCalls: state.canvasMutationCalls,
          savedRevisions: state.canvasSavedRevisions,
        }),
      );
    };
    const persistedSourceStateKey = "__learning_studio_source_backend__";
    try {
      const persisted = JSON.parse(
        sessionStorage.getItem(persistedSourceStateKey) ?? "null",
      ) as {
        programId?: string;
        workspace?: SourceWorkspace;
        versionTexts?: [string, string][];
        operations?: [string, { command: string; payload: string }][];
        mutationCalls?: typeof state.sourceMutationCalls;
        appliedEffects?: Record<string, number>;
      } | null;
      if (persisted?.programId === program.summary.id) {
        state.sourceWorkspace = persisted.workspace ?? sourceWorkspace;
        sourceVersionTexts.clear();
        for (const [versionId, text] of persisted.versionTexts ?? [])
          sourceVersionTexts.set(versionId, text);
        for (const [operationId, operation] of persisted.operations ?? [])
          sourceOperations.set(operationId, operation);
        state.sourceMutationCalls = persisted.mutationCalls ?? [];
        state.sourceAppliedEffects = persisted.appliedEffects ?? {};
      }
    } catch {
      // Malformed previous state must not weaken command validation.
    }
    const persistSourceState = () => {
      sessionStorage.setItem(
        persistedSourceStateKey,
        JSON.stringify({
          programId: program.summary.id,
          workspace: state.sourceWorkspace,
          versionTexts: [...sourceVersionTexts.entries()],
          operations: [...sourceOperations.entries()],
          mutationCalls: state.sourceMutationCalls,
          appliedEffects: state.sourceAppliedEffects,
        }),
      );
    };
    (
      window as unknown as { __LATTICE_LEARNING_STATE__: typeof state }
    ).__LATTICE_LEARNING_STATE__ = state;

    let pendingOutline: { id: string; reject: (error: unknown) => void; send: (update: LearningOutlineProgressDto) => void } | null = null;
    const outlineControl = {
      hold: false,
      makeUnresolvedDraft() {
        program.summary.status = "draft";
        program.modules.forEach((module) => module.lessons.forEach((lesson) => {
          lesson.preparation = "outline"; lesson.completed = false; lesson.blocks = []; lesson.questions = [];
        }));
        program.outlineReview = { status: "needs_repair", repairPasses: 1, updatedAt: Date.now(), contentHash: "draft-fixture", note: "Completed corrections are saved. One finding remains unresolved.", issues: [{ path: "/modules/0/lessons/0", kind: "quote", claim: "A comparison always establishes causality.", quote: "This unsupported statement is retained for review.", sourceId: source.id, message: "The reference does not support this universal claim." }] };
      },
      update(update: LearningOutlineProgressDto) { pendingOutline?.send(update); },
      fail(error: unknown) { pendingOutline?.reject(error); pendingOutline = null; },
    };
    (window as unknown as { __LATTICE_OUTLINE_TEST__: typeof outlineControl }).__LATTICE_OUTLINE_TEST__ = outlineControl;

    const copyMemory = () => JSON.parse(JSON.stringify(memory));
    const now = () => new Date().toISOString();
    const getArg = <T>(args: unknown, key: string): T =>
      (args as Record<string, T>)[key];
    const getRequest = <T>(args: unknown): T => getArg<T>(args, "request");
    const copyCanvasWorkspace = () =>
      JSON.parse(JSON.stringify(canvasWorkspace));
    const copySourceWorkspace = () =>
      JSON.parse(JSON.stringify(state.sourceWorkspace)) as SourceWorkspace;
    const copyPracticeWorkspace = () =>
      JSON.parse(JSON.stringify(practiceWorkspace)) as PracticeWorkspace;
    const findLesson = (lessonId: string) => {
      for (const module of program.modules) {
        const lesson = module.lessons.find((item) => item.id === lessonId);
        if (lesson) return { module, lesson };
      }
      throw new Error("Learning lesson not found");
    };
    const copyPracticeSession = (sessionId: string) => {
      const session = practiceSessions.get(sessionId);
      if (!session) throw new Error("Practice session not found");
      return JSON.parse(JSON.stringify(session)) as PracticeSession;
    };
    const practiceOperation = (
      command: string,
      request: Record<string, unknown>,
    ) => {
      state.practiceMutationCalls.push({ command, request });
      const operationId = request.operationId;
      if (typeof operationId !== "string" || !operationId)
        throw new Error("Practice operation ID is required");
      const payload = JSON.stringify({ command, request });
      const previous = practiceOperations.get(operationId);
      if (previous) {
        if (previous.command !== command || previous.payload !== payload)
          throw new Error(
            "Operation ID was already used with different practice data.",
          );
        return { replayed: true, operationId, payload };
      }
      return { replayed: false, operationId, payload };
    };
    const recordPracticeEffect = (
      command: string,
      operation: { operationId: string; payload: string },
    ) => {
      practiceOperations.set(operation.operationId, { command, payload: operation.payload });
      state.practiceAppliedEffects[operation.operationId] =
        (state.practiceAppliedEffects[operation.operationId] ?? 0) + 1;
      persistPracticeState();
      if (state.practiceLostResponsesRemaining > 0) {
        state.practiceLostResponsesRemaining -= 1;
        persistPracticeState();
        throw new Error("Practice mutation response was lost after persistence.");
      }
    };
    const getPracticeSession = (sessionId: string) => {
      const session = practiceSessions.get(sessionId);
      if (!session) throw new Error("Practice session not found");
      return session;
    };
    const validatePracticeScope = (request: Record<string, unknown>) => {
      if (request.programId !== program.summary.id)
        throw new Error("Learning program not found");
      if (request.sessionId != null) getPracticeSession(String(request.sessionId));
    };
    const validatePracticeRevision = (
      session: PracticeSession,
      request: Record<string, unknown>,
    ) => {
      if (session.summary.status !== "active")
        throw new Error("Submitted practice attempts are immutable.");
      if (request.expectedRevision !== session.summary.revision)
        throw new Error("Practice session changed; reload and retry.");
    };
    const syncPracticeSummary = (session: PracticeSession) => {
      const index = practiceWorkspace.sessions.findIndex(
        (item) => item.id === session.summary.id,
      );
      if (index < 0) practiceWorkspace.sessions.unshift(session.summary);
      else practiceWorkspace.sessions[index] = session.summary;
    };
    const bumpPracticeRevision = (session: PracticeSession) => {
      session.summary.revision += 1;
      session.summary.updatedAt = Date.now();
      syncPracticeSummary(session);
    };
    const practiceAidForbidden = (session: PracticeSession) => {
      if (session.summary.mode === "demonstrate")
        throw new Error("This aid is unavailable in Demonstrate mode.");
    };
    const sourceById = (sourceId: string) => {
      const result = state.sourceWorkspace.sources.find(
        (item) => item.id === sourceId,
      );
      if (!result) throw new Error("Learning source not found");
      return result;
    };
    const sourceVersionById = (
      source: SourceLibraryItem,
      versionId: string,
    ) => {
      const result = source.versions.find(
        (version) => version.id === versionId,
      );
      if (!result) throw new Error("Learning source version not found");
      return result;
    };
    const sourceSummary = (
      id: string,
      versionNumber: number,
      title: string,
      text: string,
      resolvedUrl: string | null,
      publisher: string | null,
      acquiredAt = Date.now(),
    ): SourceVersionSummary => {
      const excerpt = text.trim().slice(0, 2400);
      return {
        id,
        versionNumber,
        title,
        publisher,
        resolvedUrl,
        excerpt,
        contentSha256: (versionNumber.toString(16) + "b".repeat(63)).slice(
          0,
          64,
        ),
        wordCount: text.trim().split(/\s+/u).filter(Boolean).length,
        truncated: text.length > 64_000,
        extractionVersion: "bounded_text_v1",
        acquiredAt,
      };
    };
    const lookupSourceOperation = (
      command: string,
      request: Record<string, unknown>,
    ): { replayed: boolean; payload: string; operationId: string } => {
      const operationId = request.operationId;
      if (typeof operationId !== "string" || !operationId)
        throw new Error("Source operation ID is required");
      const payload = JSON.stringify({ command, request });
      const prior = sourceOperations.get(operationId);
      if (prior) {
        if (prior.command !== command || prior.payload !== payload)
          throw new Error(
            "Source operation ID was reused with different input",
          );
        return { replayed: true, payload, operationId };
      }
      return { replayed: false, payload, operationId };
    };
    const recordSourceEffect = (
      command: string,
      operation: { payload: string; operationId: string },
    ) => {
      sourceOperations.set(operation.operationId, {
        command,
        payload: operation.payload,
      });
      state.sourceAppliedEffects[operation.operationId] =
        (state.sourceAppliedEffects[operation.operationId] ?? 0) + 1;
    };
    const maybeLoseSourceResponse = () => {
      if (state.sourceLostResponsesRemaining > 0) {
        state.sourceLostResponsesRemaining -= 1;
        persistSourceState();
        throw new Error("Source update response was lost after persistence.");
      }
      persistSourceState();
    };
    const pushSourceMutation = (
      command: string,
      request: Record<string, unknown>,
    ) => {
      state.sourceMutationCalls.push({ command, request });
      return lookupSourceOperation(command, request);
    };
    const canvasById = (id: string) => {
      const canvas = canvasWorkspace.canvases.find((item) => item.id === id);
      if (!canvas) throw new Error("Canvas not found");
      return canvas;
    };
    const lookupCanvasOperation = (
      command: string,
      request: Record<string, unknown>,
    ): { replayed: boolean; payload: string; operationId: string } => {
      const operationId = request.operationId;
      if (typeof operationId !== "string" || !operationId)
        throw new Error("Canvas operation ID is required");
      const payload = JSON.stringify({ command, request });
      const prior = canvasOperations.get(operationId);
      if (prior) {
        if (prior.command !== command || prior.payload !== payload)
          throw new Error(
            "Canvas operation ID was reused with different input",
          );
        return { replayed: true, payload, operationId };
      }
      return { replayed: false, payload, operationId };
    };
    const recordCanvasOperation = (
      command: string,
      payload: string,
      operationId: string,
    ) => {
      canvasOperations.set(operationId, { command, payload });
    };
    const countCanvasElements = (scene: unknown) => {
      const elements = (scene as { elements?: unknown[] })?.elements;
      return Array.isArray(elements)
        ? elements.filter(
            (element) =>
              Boolean(element) &&
              typeof element === "object" &&
              (element as { isDeleted?: boolean }).isDeleted !== true,
          ).length
        : 0;
    };
    const makeSnapshot = (
      canvas: Canvas,
      id: string,
      name: string,
      revision = canvas.revision,
    ): CanvasSnapshot => ({
      id,
      canvasId: canvas.id,
      name,
      title: canvas.title,
      description: canvas.description,
      sceneJson: JSON.parse(JSON.stringify(canvas.sceneJson)),
      elementCount: canvas.elementCount,
      canvasRevision: revision,
      createdAt: Date.now(),
    });
    const eligibleCardSource = () => ({
      chunkId: source.id,
      documentId: "document-field-notes",
      fileName: "field-notes.txt",
      filePath: "/test/library/field-notes.txt",
      excerpt: source.excerpt,
      url: source.url,
    });

    const invoke = async (
      command: string,
      args?: unknown,
    ): Promise<unknown> => {
      state.calls.push({ command, args });
      if (command === "plugin:settings|get_settings") return settings;
      if (command === "plugin:health|initialize_database") return undefined;
      if (command === "plugin:model|list_downloaded_models") return [];
      if (command === "plugin:model|detect_system_capabilities")
        return { total_ram_gb: 8, cpu_cores: 8, cpu_architecture: "e2e", gpu_type: "none", gpu_acceleration: "none", vram_gb: null, available_disk_gb: 10 };
      if (command === "plugin:conversation|list_conversations_explorer" || command === "plugin:conversation|list_conversations")
        return { conversations: [], total: 0 };
      if (command === "plugin:file|get_index_progress")
        return { totalFiles: 0, processed: 0, failed: 0, status: "idle", percentage: 0, paused: false, failures: [] };
      if (
        [
          "plugin:download|list_downloads",
          "plugin:file|get_indexed_folders",
          "plugin:file|list_custom_collections",
          "plugin:corpus-shape|list_clusters",
          "plugin:conversation|list_conversation_spaces",
        ].includes(command)
      )
        return [];
      if (command === "plugin:conversation|list_space_documents") return [];
      if (command === "plugin:file|list_all_documents") return [];
      if (command === "plugin:learning|cancel_learning_outline") {
        if (!pendingOutline || pendingOutline.id !== getArg<string>(args, "requestId")) return false;
        pendingOutline.reject({ code: "SERVICE_NOT_AVAILABLE", message: "Course generation cancelled. Your inputs are kept." });
        pendingOutline = null;
        return true;
      }
      if (command === "plugin:learning|generate_learning_program") {
        if (outlineControl.hold) {
          const channel = getArg<{ id: number }>(args, "onProgress");
          let index = 0;
          return new Promise((_resolve, reject) => {
            pendingOutline = { id: getArg<string>(args, "requestId"), reject, send: (message) => {
              callbacks.get(channel.id)?.({ index: index++, message });
            } };
            pendingOutline.send({ stage: "reading_sources", elapsedSeconds: 0, stageSeconds: 0, responseCharacters: 0, modelName: null });
          });
        }
        const request = getArg<{ goal: string; priorKnowledge: string; minutesPerSession: number }>(args, "request");
        program.summary.title = request.goal;
        program.summary.goal = request.goal;
        program.summary.status = "draft";
        program.summary.revision = 0;
        program.summary.completedLessons = 0;
        program.priorKnowledge = request.priorKnowledge;
        program.minutesPerSession = request.minutesPerSession;
        program.sources = [];
        program.modules.forEach((module) => module.lessons.forEach((lesson) => {
          lesson.preparation = "outline";
          lesson.completed = false;
          lesson.blocks = [];
          lesson.questions = [];
        }));
        return program;
      }
      if (command === "plugin:learning|repair_learning_outline") {
        const request = getArg<{ programId: string; expectedRevision: number }>(args, "request");
        if (request.programId !== program.summary.id || request.expectedRevision !== program.summary.revision) throw new Error("Draft changed");
        program.summary.revision += 1;
        program.modules[0].lessons[0].objective = "Compare observations and explain why they alone do not establish causality.";
        program.outlineReview = { status: "passed", repairPasses: 2, updatedAt: Date.now(), contentHash: "repaired-fixture", issues: [], note: "Outline checks passed for this revision. Lesson content is checked separately." };
        return program;
      }
      if (command === "plugin:learning|list_learning_programs")
        return [program.summary];
      if (command === "plugin:learning|get_learning_program") return program;
      if (command === "plugin:learning|get_learning_lesson_evidence") return {
        policy: "lesson-evidence-v2", checkedAt: Date.UTC(2026, 9, 4), checkerModel: "Evidence fixture",
        retrievalMode: "hybrid", embeddingModel: "Embedding fixture", contentSha256: "a".repeat(64),
        claimCount: 14, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
        teachingClaims: [{ sectionIndex: 0, claim: "A careful comparison records the chosen measure and observation period.",
          reason: "The saved reference supports this teaching claim.", supportingQuote: source.excerpt,
          passages: [{ sourceVersionId: source.id, title: source.title, url: source.url, text: source.excerpt,
            startByte: 0, endByte: source.excerpt.length, retrievalKind: "hybrid" }] }],
      };
      if (command === "plugin:learning|get_learning_memory")
        return copyMemory();
      if (command === "plugin:learning|get_learning_practice_workspace") {
        const programId = String(getArg(args, "id"));
        if (programId !== program.summary.id)
          throw new Error("Learning program not found");
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|get_learning_practice_session") {
        return copyPracticeSession(String(getArg(args, "id")));
      }
      if (command === "plugin:learning|start_learning_practice_session") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          if (request.programId !== program.summary.id)
            throw new Error("Learning program not found");
          if (program.summary.status !== "active")
            throw new Error("Accept the learning program before starting practice.");
          const { lesson } = findLesson(String(request.lessonId));
          if (lesson.preparation !== "ready")
            throw new Error("Prepare this lesson before starting practice.");
          if (request.expectedProgramRevision !== program.summary.revision)
            throw new Error("Program changed; reload and retry.");
          if (practiceSessions.has(String(request.sessionId)))
            throw new Error("Practice session ID is already in use.");
          const timestamp = Date.now();
          const sourceVersionIds = [...new Set(lesson.blocks.flatMap((block) => block.sourceIds))];
          const session: PracticeSession = {
            summary: {
              id: String(request.sessionId),
              lessonId: lesson.id,
              lessonTitle: lesson.title,
              status: "active",
              mode: String(request.mode) as PracticeMode,
              revision: 0,
              artifactRevision: 0,
              sourceVersionIds,
              createdAt: timestamp,
              updatedAt: timestamp,
              submittedAt: null,
              gradeStatus: null,
              taskKind: request.taskKind === "guided" ? "guided" : "independent",
              revisesSessionId: typeof request.revisesSessionId === "string" ? request.revisesSessionId : null,
            },
            taskPrompt: "Compare the same measure across the stated observation period and explain one limit of the evidence.",
            lessonObjective: lesson.objective,
            rubric: JSON.parse(JSON.stringify(practiceRubric)) as PracticeCriterion[],
            artifact: { revision: 0, text: "", sha256: "e".repeat(64), updatedAt: timestamp },
            assistance: [],
            tutorTurns: [],
            proposals: [],
            revealedSolution: null,
            revealedSolutionCitations: [],
            result: null,
          };
          const task = lesson.blocks.find((block) => block.kind === (request.taskKind === "guided" ? "guided_practice" : "independent_practice"));
          if (task) session.taskPrompt = task.body;
          if (request.revisesSessionId) {
            const parent = practiceSessions.get(String(request.revisesSessionId));
            if (!parent || parent.summary.status !== "submitted" || parent.summary.lessonId !== lesson.id) throw new Error("Only a submitted attempt from this lesson can be revised.");
            session.taskPrompt = parent.taskPrompt;
            session.artifact.text = parent.artifact.text;
            session.rubric = JSON.parse(JSON.stringify(parent.rubric)) as PracticeCriterion[];
          }
          practiceSessions.set(session.summary.id, session);
          syncPracticeSummary(session);
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|save_learning_practice_artifact") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          const text = String(request.text);
          if (text.length > 64_000)
            throw new Error("Practice response exceeds the saved text limit.");
          if (text !== session.artifact.text) {
            session.artifact = {
              revision: session.artifact.revision + 1,
              text,
              sha256: `${text.length.toString(16)}${"a".repeat(63)}`.slice(0, 64),
              updatedAt: Date.now(),
            };
            session.summary.artifactRevision = session.artifact.revision;
            bumpPracticeRevision(session);
          }
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|change_learning_practice_mode") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          if (request.mode !== session.summary.mode) {
            const previousMode = session.summary.mode;
            session.summary.mode = String(request.mode) as PracticeMode;
            session.assistance.push({
              id: crypto.randomUUID(),
              kind: "mode_changed",
              mode: session.summary.mode,
              artifactRevision: session.artifact.revision,
              details: { from: previousMode, to: session.summary.mode },
              createdAt: Date.now(),
            });
            bumpPracticeRevision(session);
          }
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|open_learning_practice_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          if (session.summary.mode === "demonstrate")
            throw new Error("Source access is unavailable in Demonstrate mode.");
          const sourceId = String(request.sourceId);
          const versionId = String(request.versionId);
          if (!session.summary.sourceVersionIds.includes(versionId))
            throw new Error("Source version is not attached to this practice attempt.");
          const source = state.sourceWorkspace.sources.find((item) => item.id === sourceId);
          if (!source || !source.versions.some((version) => version.id === versionId))
            throw new Error("Citation does not match an exact saved source version.");
          session.assistance.push({
            id: crypto.randomUUID(),
            kind: "source_opened",
            mode: session.summary.mode,
            artifactRevision: session.artifact.revision,
            details: { sourceId, versionId },
            createdAt: Date.now(),
          });
          bumpPracticeRevision(session);
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|request_learning_tutor_response") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          practiceAidForbidden(session);
          const quote = "Record the same measure across the observation period.";
          const versionId = session.summary.sourceVersionIds[0];
          const sourceId = state.sourceWorkspace.sources.find((item) =>
            item.versions.some((version) => version.id === versionId),
          )?.id;
          if (!sourceId)
            throw new Error("Frozen source version is unavailable in the source library.");
          const citation: PracticeCitation = {
            sourceId,
            versionId,
            quote,
          };
          const frozenText = sourceVersionTexts.get(versionId);
          if (
            !program.sources.some((source) => source.id === versionId) ||
            !frozenText?.includes(quote)
          )
            throw new Error("Tutor citation must reference a frozen source version.");
          const hintLevel = request.hintLevel == null ? null : String(request.hintLevel) as PracticeHintLevel;
          const requestKind = String(request.requestKind) as "hint" | "question" | "critique";
          if (!["hint", "question", "critique"].includes(requestKind))
            throw new Error("Tutor request kind is invalid.");
          if (requestKind === "hint" && !hintLevel)
            throw new Error("Choose an explicit hint level.");
          const response = hintLevel
            ? `Hint ${session.tutorTurns.filter((turn) => turn.hintLevel !== null).length + 1}: start by naming the measure. ${quote}`
            : `The saved excerpt emphasizes a measure and an observation period. ${quote}`;
          const turnId = crypto.randomUUID();
          const proposals: PracticeProposal[] = requestKind === "hint" ? [] : [
            { id: crypto.randomUUID(), kind: "misconception", text: "Check whether the comparison uses the same measure.", evidenceQuote: quote, tutorTurnId: turnId, status: "pending", createdAt: Date.now(), decidedAt: null },
            { id: crypto.randomUUID(), kind: "follow_up", text: "Compare a second observation period.", evidenceQuote: quote, tutorTurnId: turnId, status: "pending", createdAt: Date.now(), decidedAt: null },
          ];
          session.tutorTurns.push({
            id: turnId,
            prompt: String(request.prompt),
            requestKind,
            response,
            hintLevel,
            citations: [citation],
            proposalIds: proposals.map((proposal) => proposal.id),
            modelName: "Configured local model",
            createdAt: Date.now(),
          });
          session.proposals.push(...proposals);
          if (hintLevel) {
            session.assistance.push({
              id: crypto.randomUUID(),
              kind: "hint",
              mode: session.summary.mode,
              artifactRevision: session.artifact.revision,
              details: { hintLevel },
              createdAt: Date.now(),
            });
          } else {
            session.assistance.push({
              id: crypto.randomUUID(),
              kind: "tutor_response",
              mode: session.summary.mode,
              artifactRevision: session.artifact.revision,
              details: { requestKind },
              createdAt: Date.now(),
            });
          }
          bumpPracticeRevision(session);
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|reveal_learning_practice_solution") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          if (session.summary.mode === "demonstrate")
            throw new Error("Solution reveal is unavailable in Demonstrate mode.");
          const versionId = session.summary.sourceVersionIds[0];
          const quote = "Record the same measure across the observation period.";
          if (!sourceVersionTexts.get(versionId)?.includes(quote))
            throw new Error("Solution citation must match its frozen source text.");
          const sourceId = state.sourceWorkspace.sources.find((item) =>
            item.versions.some((version) => version.id === versionId),
          )?.id;
          if (!sourceId)
            throw new Error("Frozen source version is unavailable in the source library.");
          session.revealedSolution = `Compare the same measure over the full observation period, then describe limits in the available cases. ${quote}`;
          session.revealedSolutionCitations = [{ sourceId, versionId, quote }];
          session.assistance.push({
            id: crypto.randomUUID(),
            kind: "solution_revealed",
            mode: session.summary.mode,
            artifactRevision: session.artifact.revision,
            details: {},
            createdAt: Date.now(),
          });
          bumpPracticeRevision(session);
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|submit_learning_practice_attempt") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          const submittedAt = Date.now();
          const assistanceKinds = [...new Set(session.assistance.map((event) => event.kind))];
          const criteria = session.rubric.map((criterion) => ({
            criterionId: criterion.id,
            dimension: criterion.dimension,
            score: session.artifact.text.trim() ? 2 : null,
            maxPoints: criterion.maxPoints,
            observation: session.artifact.text.trim() ? "The saved response addresses this criterion provisionally." : "No response text was available for this criterion.",
            evidenceQuote: session.artifact.text.trim() ? session.artifact.text.trim().slice(0, Math.min(80, session.artifact.text.trim().length)) : null,
          }));
          const dimensions: PracticeCriterion["dimension"][] = ["recall", "explanation", "application", "transfer"];
          const evidence = dimensions.map((dimension) => ({
            dimension,
            observed: session.artifact.text.trim().length > 0,
            observation: session.artifact.text.trim() ? `Provisional evidence recorded for ${dimension}.` : `No ${dimension} evidence was observed.`,
            evidenceQuote: session.artifact.text.trim() ? session.artifact.text.trim().slice(0, Math.min(80, session.artifact.text.trim().length)) : null,
            assistanceKinds,
          }));
          session.summary.status = "submitted";
          session.summary.submittedAt = submittedAt;
          session.summary.gradeStatus = "provisional";
          session.result = {
            gradeStatus: "provisional",
            artifactRevision: session.artifact.revision,
            artifactText: session.artifact.text,
            rubric: JSON.parse(JSON.stringify(session.rubric)) as PracticeCriterion[],
            criteria,
            evidence,
            modeAtSubmission: session.summary.mode,
            assistance: JSON.parse(JSON.stringify(session.assistance)) as PracticeAssistanceEvent[],
            graderModel: "Configured local model",
            submittedAt,
          };
          bumpPracticeRevision(session);
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (
        command === "plugin:learning|accept_learning_practice_proposal" ||
        command === "plugin:learning|reject_learning_practice_proposal"
      ) {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = practiceOperation(command, request);
        if (!operation.replayed) {
          validatePracticeScope(request);
          const session = getPracticeSession(String(request.sessionId));
          validatePracticeRevision(session, request);
          const proposal = session.proposals.find((item) => item.id === request.proposalId);
          if (!proposal || proposal.status !== "pending")
            throw new Error("Pending proposal not found.");
          proposal.status = command.includes("accept_") ? "accepted" : "rejected";
          proposal.decidedAt = Date.now();
          bumpPracticeRevision(session);
          recordPracticeEffect(command, operation);
        }
        return copyPracticeWorkspace();
      }
      if (command === "plugin:learning|get_learning_canvas_workspace")
        return copyCanvasWorkspace();
      if (command === "plugin:learning|create_learning_canvas") {
        const request = getRequest<Record<string, unknown>>(args);
        state.canvasMutationCalls.push({ command, request });
        const operation = lookupCanvasOperation(command, request);
        if (!operation.replayed) {
          if (program.summary.status !== "active")
            throw new Error(
              "Accept the learning program before editing canvases.",
            );
          if (
            canvasWorkspace.canvases.some(
              (item) => item.id === request.canvasId,
            )
          )
            throw new Error("Canvas ID is already in use.");
          const timestamp = Date.now();
          canvasWorkspace.canvases.push({
            id: String(request.canvasId),
            programId: String(request.programId),
            lessonId: request.lessonId ? String(request.lessonId) : null,
            title: String(request.title).trim(),
            description: String(request.description).trim(),
            sceneJson: JSON.parse(JSON.stringify(request.sceneJson)),
            elementCount: countCanvasElements(request.sceneJson),
            revision: 0,
            createdAt: timestamp,
            updatedAt: timestamp,
            snapshots: [],
          });
          recordCanvasOperation(
            command,
            operation.payload,
            operation.operationId,
          );
          persistCanvasState();
        }
        return copyCanvasWorkspace();
      }
      if (command === "plugin:learning|save_learning_canvas") {
        const request = getRequest<Record<string, unknown>>(args);
        state.canvasMutationCalls.push({ command, request });
        const operation = lookupCanvasOperation(command, request);
        if (!operation.replayed) {
          const canvas = canvasById(String(request.canvasId));
          if (request.expectedRevision !== canvas.revision)
            throw new Error("Canvas changed; reload and retry.");
          if (program.summary.status !== "active")
            throw new Error(
              "Accept the learning program before editing canvases.",
            );
          canvas.title = String(request.title).trim();
          canvas.description = String(request.description).trim();
          canvas.sceneJson = JSON.parse(JSON.stringify(request.sceneJson));
          canvas.elementCount = countCanvasElements(request.sceneJson);
          canvas.revision += 1;
          canvas.updatedAt = Date.now();
          state.canvasSavedRevisions.push(canvas.revision);
          recordCanvasOperation(
            command,
            operation.payload,
            operation.operationId,
          );
          persistCanvasState();
          if (state.canvasSaveLostResponsesRemaining > 0) {
            state.canvasSaveLostResponsesRemaining -= 1;
            await state.lostSaveResponseGate;
            throw new Error("Canvas save response was lost after persistence.");
          }
        }
        return copyCanvasWorkspace();
      }
      if (command === "plugin:learning|create_learning_canvas_snapshot") {
        const request = getRequest<Record<string, unknown>>(args);
        state.canvasMutationCalls.push({ command, request });
        const operation = lookupCanvasOperation(command, request);
        if (!operation.replayed) {
          const canvas = canvasById(String(request.canvasId));
          if (request.expectedRevision !== canvas.revision)
            throw new Error("Canvas changed; reload and retry.");
          if (canvas.snapshots.some((item) => item.id === request.snapshotId))
            throw new Error("Snapshot ID is already in use.");
          canvas.snapshots.push(
            makeSnapshot(
              canvas,
              String(request.snapshotId),
              String(request.name).trim(),
            ),
          );
          recordCanvasOperation(
            command,
            operation.payload,
            operation.operationId,
          );
          persistCanvasState();
        }
        return copyCanvasWorkspace();
      }
      if (command === "plugin:learning|restore_learning_canvas_snapshot") {
        const request = getRequest<Record<string, unknown>>(args);
        state.canvasMutationCalls.push({ command, request });
        const operation = lookupCanvasOperation(command, request);
        if (!operation.replayed) {
          const canvas = canvasById(String(request.canvasId));
          if (request.expectedRevision !== canvas.revision)
            throw new Error("Canvas changed; reload and retry.");
          const target = canvas.snapshots.find(
            (item) => item.id === request.snapshotId,
          );
          if (!target) throw new Error("Canvas snapshot not found.");
          canvas.snapshots.push(
            makeSnapshot(
              canvas,
              String(request.preRestoreSnapshotId),
              `Before restore · ${String(request.operationId).slice(0, 8)}`,
            ),
          );
          canvas.title = target.title;
          canvas.description = target.description;
          canvas.sceneJson = JSON.parse(JSON.stringify(target.sceneJson));
          canvas.elementCount = target.elementCount;
          canvas.revision += 1;
          canvas.updatedAt = Date.now();
          recordCanvasOperation(
            command,
            operation.payload,
            operation.operationId,
          );
          persistCanvasState();
        }
        return copyCanvasWorkspace();
      }
      if (command === "plugin:learning|get_learning_source_workspace") {
        const programId = getArg<string>(args, "id");
        return programId === sourceWorkspace.programId
          ? copySourceWorkspace()
          : { programId, sources: [] };
      }
      if (command === "plugin:learning|get_learning_source_version") {
        const request = getRequest<{
          programId: string;
          sourceId: string;
          versionId: string;
        }>(args);
        if (request.programId !== sourceWorkspace.programId)
          throw new Error("Learning source not found");
        const item = sourceById(request.sourceId);
        const version = sourceVersionById(item, request.versionId);
        return {
          sourceId: item.id,
          version: JSON.parse(JSON.stringify(version)),
          fullText: sourceVersionTexts.get(version.id) ?? version.excerpt,
          usage:
            version.id === sourceV1.id
              ? [
                  {
                    lessonId: lesson.id,
                    lessonTitle: lesson.title,
                    referenceKind: "lesson_block",
                    referenceTitle: "Choose a measure",
                  },
                ]
              : [],
        } satisfies SourceVersionDetail;
      }
      if (command === "plugin:learning|search_learning_sources") {
        const request = getRequest<{
          programId: string;
          query: string;
          limit: number;
        }>(args);
        const query = request.query.trim().toLocaleLowerCase();
        const results =
          request.programId !== sourceWorkspace.programId || !query
            ? []
            : sourceWorkspace.sources
                .flatMap((item) =>
                  item.versions.flatMap((version) => {
                    const text =
                      sourceVersionTexts.get(version.id) ?? version.excerpt;
                    const matchIndex = text.toLocaleLowerCase().indexOf(query);
                    if (matchIndex < 0) return [];
                    const start = Math.max(0, matchIndex - 60);
                    const end = Math.min(
                      text.length,
                      matchIndex + query.length + 100,
                    );
                    return [
                      {
                        sourceId: item.id,
                        versionId: version.id,
                        title: version.title,
                        excerpt: text.slice(start, end),
                      },
                    ];
                  }),
                )
                .slice(0, Math.max(0, Math.min(request.limit, 50)));
        return results;
      }
      if (command === "plugin:learning|add_learning_web_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = pushSourceMutation(command, request);
        if (!operation.replayed) {
          if (request.programId !== sourceWorkspace.programId)
            throw new Error("Learning program not found");
          if (program.summary.status !== "active")
            throw new Error(
              "Accept the learning program before adding sources.",
            );
          const id = String(request.sourceId);
          if (sourceWorkspace.sources.some((item) => item.id === id))
            throw new Error("Source ID is already in use.");
          const url = String(request.url);
          const versionId = String(request.versionId);
          const title = "Supplementary field notes";
          const text =
            "ADDED_SOURCE_SENTINEL: The second edition records the selected measure and observation interval.";
          const version = sourceSummary(
            versionId,
            1,
            title,
            text,
            url,
            "Field Notes Archive",
          );
          const timestamp = Date.now();
          sourceVersionTexts.set(version.id, text);
          state.sourceWorkspace.sources.push({
            id,
            kind: "web",
            origin: url,
            requestedUrl: url,
            freshnessPolicy: String(request.freshnessPolicy) as SourcePolicy,
            activeVersionId: version.id,
            pendingVersionId: null,
            revision: 0,
            activeVersion: version,
            pendingVersion: null,
            versions: [version],
            latestCheck: null,
            checks: [],
            createdAt: timestamp,
            updatedAt: timestamp,
          });
          recordSourceEffect(command, operation);
          maybeLoseSourceResponse();
        }
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|add_learning_document_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = pushSourceMutation(command, request);
        if (!operation.replayed) {
          if (request.programId !== sourceWorkspace.programId)
            throw new Error("Learning program not found");
          const id = String(request.sourceId);
          if (sourceWorkspace.sources.some((item) => item.id === id))
            throw new Error("Source ID is already in use.");
          const text =
            "DOCUMENT_SOURCE_SENTINEL: Selected local document snapshot.";
          const version = sourceSummary(
            String(request.versionId),
            1,
            "Field notes.txt",
            text,
            null,
            null,
          );
          const timestamp = Date.now();
          sourceVersionTexts.set(version.id, text);
          state.sourceWorkspace.sources.push({
            id,
            kind: "document",
            origin: String(request.documentId),
            requestedUrl: null,
            freshnessPolicy: "fixed",
            activeVersionId: version.id,
            pendingVersionId: null,
            revision: 0,
            activeVersion: version,
            pendingVersion: null,
            versions: [version],
            latestCheck: null,
            checks: [],
            createdAt: timestamp,
            updatedAt: timestamp,
          });
          recordSourceEffect(command, operation);
          maybeLoseSourceResponse();
        }
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|add_learning_text_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = pushSourceMutation(command, request);
        if (!operation.replayed) {
          if (request.programId !== sourceWorkspace.programId)
            throw new Error("Learning program not found");
          const id = String(request.sourceId);
          if (sourceWorkspace.sources.some((item) => item.id === id))
            throw new Error("Source ID is already in use.");
          const text = String(request.text);
          const version = sourceSummary(
            String(request.versionId),
            1,
            String(request.title),
            text,
            null,
            request.publisher ? String(request.publisher) : null,
          );
          const timestamp = Date.now();
          sourceVersionTexts.set(version.id, text);
          state.sourceWorkspace.sources.push({
            id,
            kind: "pasted",
            origin: "Pasted text",
            requestedUrl: null,
            freshnessPolicy: "fixed",
            activeVersionId: version.id,
            pendingVersionId: null,
            revision: 0,
            activeVersion: version,
            pendingVersion: null,
            versions: [version],
            latestCheck: null,
            checks: [],
            createdAt: timestamp,
            updatedAt: timestamp,
          });
          recordSourceEffect(command, operation);
          maybeLoseSourceResponse();
        }
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|refresh_learning_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = pushSourceMutation(command, request);
        if (!operation.replayed) {
          if (request.programId !== sourceWorkspace.programId)
            throw new Error("Learning program not found");
          const item = sourceById(String(request.sourceId));
          if (request.expectedRevision !== item.revision)
            throw new Error(
              "Source revision changed elsewhere; reload and retry.",
            );
          if (item.kind !== "web")
            throw new Error("Only web sources can be refreshed.");
          const checkedAt = Date.now();
          const active = item.activeVersion;
          let check: SourceCheck;
          if (state.nextSourceRefreshOutcome === "changed") {
            const text =
              "UPDATED_SOURCE_SENTINEL: A revised edition states the same measure and a newly extended observation interval.";
            const digest = "c".repeat(64);
            let pending = item.versions.find(
              (version) => version.contentSha256 === digest,
            );
            if (!pending) {
              const versionNumber =
                Math.max(0, ...item.versions.map((version) => version.versionNumber)) +
                1;
              const versionId = `source-version-${item.id}-${versionNumber}`;
              pending = sourceSummary(
                versionId,
                versionNumber,
                active?.title ?? "Supplementary field notes",
                text,
                item.requestedUrl,
                active?.publisher ?? null,
                checkedAt,
              );
              pending.contentSha256 = digest;
              sourceVersionTexts.set(versionId, text);
              item.versions.unshift(pending);
            }
            if (item.pendingVersionId !== pending.id) {
              item.pendingVersionId = pending.id;
              item.pendingVersion = pending;
              item.revision += 1;
            }
            check = {
              operationId: operation.operationId,
              status: "update_available",
              checkedAt,
              activeDigest: active?.contentSha256 ?? null,
              pendingVersionId: pending.id,
              message: null,
            };
          } else if (state.nextSourceRefreshOutcome === "failed") {
            check = {
              operationId: operation.operationId,
              status: "failed",
              checkedAt,
              activeDigest: active?.contentSha256 ?? null,
              pendingVersionId: item.pendingVersionId,
              message: "Simulated offline source check.",
            };
          } else {
            // A refresh that matches the active content invalidates an older
            // pending pointer. Keep the immutable version in history, but
            // advance the aggregate revision because the pointer changed.
            const clearedPending = item.pendingVersionId !== null;
            if (clearedPending) {
              item.pendingVersionId = null;
              item.pendingVersion = null;
              item.revision += 1;
            }
            check = {
              operationId: operation.operationId,
              status: "unchanged",
              checkedAt,
              activeDigest: active?.contentSha256 ?? null,
              pendingVersionId: null,
              message: null,
            };
          }
          item.checks.unshift(check);
          item.latestCheck = check;
          item.updatedAt = checkedAt;
          recordSourceEffect(command, operation);
          maybeLoseSourceResponse();
        }
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|adopt_learning_source_version") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = pushSourceMutation(command, request);
        if (!operation.replayed) {
          if (request.programId !== sourceWorkspace.programId)
            throw new Error("Learning program not found");
          const item = sourceById(String(request.sourceId));
          if (request.expectedRevision !== item.revision)
            throw new Error(
              "Source revision changed elsewhere; reload and retry.",
            );
          const version = sourceVersionById(item, String(request.versionId));
          item.activeVersionId = version.id;
          item.activeVersion = version;
          if (item.pendingVersionId === version.id) {
            item.pendingVersionId = null;
            item.pendingVersion = null;
          }
          item.revision += 1;
          item.updatedAt = Date.now();
          recordSourceEffect(command, operation);
          maybeLoseSourceResponse();
        }
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|update_learning_source_policy") {
        const request = getRequest<Record<string, unknown>>(args);
        const operation = pushSourceMutation(command, request);
        if (!operation.replayed) {
          if (request.programId !== sourceWorkspace.programId)
            throw new Error("Learning program not found");
          const item = sourceById(String(request.sourceId));
          if (request.expectedRevision !== item.revision)
            throw new Error(
              "Source revision changed elsewhere; reload and retry.",
            );
          const policy = String(request.freshnessPolicy) as SourcePolicy;
          if (policy !== item.freshnessPolicy) {
            item.freshnessPolicy = policy;
            item.revision += 1;
            item.updatedAt = Date.now();
          }
          recordSourceEffect(command, operation);
          maybeLoseSourceResponse();
        }
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|ensure_learning_lesson_note") {
        if (!memory.lessonNotes.length) {
          const timestamp = now();
          memory.lessonNotes.push({
            lessonId: lesson.id,
            note: {
              id: "note-learning-1",
              revision: 0,
              title: lesson.title,
              journalId: memory.journalId,
              content: "",
              linkedDocumentIds: [],
              linkedConversationIds: [],
              highlights: [],
              stickyNotes: [],
              conversationSnapshots: [],
              sources: [],
              createdAt: timestamp,
              updatedAt: timestamp,
            },
          });
        }
        return copyMemory();
      }
      if (command === "plugin:dailynotes|update_workspace_note") {
        await new Promise((resolve) => window.setTimeout(resolve, 125));
        const note = getArg<WorkspaceNoteDto>(args, "note");
        const saved = { ...note, revision: note.revision + 1, updatedAt: now() };
        const linked = memory.lessonNotes.find(
          (item) => item.note.id === note.id,
        );
        if (!linked) throw new Error("Unknown notebook note");
        if (linked.note.revision !== note.revision)
          throw new Error("This page changed elsewhere; keep your draft and reload.");
        linked.note = saved;
        state.noteWrites += 1;
        return saved;
      }
      if (command === "plugin:learning|generate_learning_card_drafts") {
        if (state.generationFailuresRemaining > 0) {
          state.generationFailuresRemaining -= 1;
          throw new Error("The draft service is temporarily unavailable.");
        }
        const request = getRequest<{ count: number; lessonId: string }>(args);
        for (let index = 0; index < request.count; index += 1) {
          memory.drafts.push({
            id: `draft-learning-${index + 1}`,
            lessonId: request.lessonId,
            question: `Which details define a careful comparison, version ${index + 1}?`,
            answer:
              index === 0
                ? "CARD_ANSWER_SENTINEL: Record the measure and observation period."
                : `Record the measure and observation period (${index + 1}).`,
            explanation:
              "Those details make the basis of the comparison explicit.",
            sourceIds: [source.id],
            origin: "generated",
            createdAt: 1_790_000_000_001 + index,
          });
        }
        return copyMemory();
      }
      if (command === "plugin:learning|save_learning_card_draft") {
        const request = getRequest<{
          draftId: string;
          question: string;
          answer: string;
          explanation: string;
          sourceIds: string[];
        }>(args);
        const draft = memory.drafts.find((item) => item.id === request.draftId);
        if (!draft) throw new Error("Draft not found");
        Object.assign(draft, {
          question: request.question,
          answer: request.answer,
          explanation: request.explanation,
          sourceIds: request.sourceIds,
        });
        return copyMemory();
      }
      if (command === "plugin:learning|accept_learning_card_draft") {
        const request = getRequest<{ draftId: string }>(args);
        const index = memory.drafts.findIndex(
          (item) => item.id === request.draftId,
        );
        if (index < 0) throw new Error("Draft not found");
        const [draft] = memory.drafts.splice(index, 1);
        const cardId = `accepted-${draft.id}`;
        const card: StudyCardDto = {
          id: cardId,
          format: "question_answer",
          schedulerVersion: "expanding_v1",
          deckId: "deck-learning-1",
          question: draft.question,
          answer: draft.answer,
          options: [],
          correctIndex: 0,
          explanation: draft.explanation,
          source: eligibleCardSource(),
          citations: [eligibleCardSource()],
          topic: lesson.title,
          dueAt: 0,
          intervalDays: 0,
          reviewCount: 0,
          lapses: 0,
        };
        const deck = memory.studyDeck ?? {
          id: "deck-learning-1",
          title: program.summary.title,
          focus: program.summary.goal,
          modelName: "Configured local model",
          createdAt: 1_790_000_000_002,
          cards: [],
        };
        memory.studyDeck = deck;
        deck.cards.push(card);
        memory.acceptedCards.push({
          cardId,
          lessonId: draft.lessonId,
          origin: "generated",
          sourceIds: [...draft.sourceIds],
          acceptedAt: 1_790_000_000_003,
        });
        memory.dueCount = deck.cards.filter(
          (item) => item.dueAt <= Date.now(),
        ).length;
        return copyMemory();
      }
      if (command === "plugin:learning|discard_learning_card_draft") {
        const request = getRequest<{ draftId: string }>(args);
        memory.drafts = memory.drafts.filter(
          (item) => item.id !== request.draftId,
        );
        return copyMemory();
      }
      // Studio's Flashcards section lists every deck; the backend includes
      // program decks, so the program's recall deck shows up here too.
      if (command === "plugin:study|list_study_decks") {
        const deck = memory.studyDeck;
        if (!deck) return [];
        const summary: StudyDeckSummaryDto = {
          id: deck.id,
          title: deck.title,
          focus: deck.focus,
          createdAt: deck.createdAt,
          cardCount: deck.cards.length,
          dueCount: deck.cards.filter((item) => item.dueAt <= Date.now()).length,
          quizAttempts: 0,
          quizCorrect: 0,
        };
        return [summary];
      }
      if (command === "plugin:study|review_study_card") {
        const request = getRequest<{
          reviewId: string;
          cardId: string;
          expectedReviews: number;
          selectedOption: number | null;
          rating: string;
        }>(args);
        state.reviewRequests.push(request);
        if (state.appliedReviewIds[request.reviewId])
          return state.appliedReviewIds[request.reviewId];
        if (state.reviewFailuresRemaining > 0) {
          state.reviewFailuresRemaining -= 1;
          throw new Error("Connection interrupted while saving review.");
        }
        const deck = memory.studyDeck;
        const card = deck?.cards.find((item) => item.id === request.cardId);
        if (!deck || !card) throw new Error("Review card not found");
        if (card.reviewCount !== request.expectedReviews)
          throw new Error("Review state changed");
        card.reviewCount += 1;
        card.intervalDays =
          request.rating === "good" ? 1 : request.rating === "easy" ? 2 : 0;
        card.dueAt = Date.now() + card.intervalDays * 24 * 60 * 60 * 1000;
        if (request.rating === "again") card.lapses += 1;
        memory.dueCount = deck.cards.filter(
          (item) => item.dueAt <= Date.now(),
        ).length;
        const response = JSON.parse(JSON.stringify(card));
        state.appliedReviewIds[request.reviewId] = response;
        return response;
      }
      if (command === "plugin:learning|get_learning_assessment_workspace")
        return state.assessmentWorkspace;
      if (command === "plugin:learning|start_learning_assessment_form") {
        const request = getRequest<Record<string, unknown>>(args);
        const blueprint = (state.assessmentWorkspace.blueprints as Array<Record<string, unknown>>).find((item) => item.id === request.blueprintId);
        if (!blueprint) throw new Error("Accepted assessment blueprint not found.");
        const id = String(request.formId);
        const form = {
          id, programId: program.summary.id, blueprintId: blueprint.id,
          blueprintRevision: blueprint.revision, purpose: blueprint.purpose,
          title: blueprint.title, instructions: blueprint.instructions,
          status: "active", revision: 0, createdAt: Date.now(), submittedAt: null,
          retakeOfFormId: request.retakeOfFormId ?? null,
          items: [{ id: "assessment-item-e2e", format: "short_answer", prompt: "Which details make the comparison interpretable?", options: [], selectedIndex: null, textResponse: "", artifactJson: null, orderedValues: [], artifactKind: null, previouslyExposed: false, rubric: [] }],
          result: null,
        };
        state.assessmentForms.set(id, form);
        (state.assessmentWorkspace.forms as Array<Record<string, unknown>>).unshift({ id, blueprintId: blueprint.id, blueprintRevision: blueprint.revision, programId: program.summary.id, purpose: blueprint.purpose, title: blueprint.title, status: "active", revision: 0, gradeStatus: null, score: null, createdAt: form.createdAt, submittedAt: null, retakeOfFormId: form.retakeOfFormId });
        return form;
      }
      if (command === "plugin:learning|get_learning_assessment_form") {
        const id = getArg<string>(args, "id");
        const form = state.assessmentForms.get(id);
        if (!form) throw new Error("Assessment form not found.");
        return form;
      }
      if (command === "plugin:learning|save_learning_assessment_response") {
        const request = getRequest<Record<string, unknown>>(args);
        const form = state.assessmentForms.get(String(request.formId));
        if (!form || form.status !== "active") throw new Error("Assessment form is no longer active.");
        const response = request.response as Record<string, unknown>;
        const item = (form.items as Array<Record<string, unknown>>).find((entry) => entry.id === response.itemId);
        if (!item) throw new Error("Assessment item not found.");
        Object.assign(item, { selectedIndex: response.selectedIndex, textResponse: response.text, orderedValues: response.orderedValues, artifactJson: response.artifactJson });
        form.revision = Number(form.revision) + 1;
        const summary = (state.assessmentWorkspace.forms as Array<Record<string, unknown>>).find((entry) => entry.id === form.id);
        if (summary) summary.revision = form.revision;
        return form;
      }
      if (command === "plugin:learning|interrupt_learning_assessment_form" || command === "plugin:learning|submit_learning_assessment_form") {
        const request = getRequest<Record<string, unknown>>(args);
        const form = state.assessmentForms.get(String(request.formId));
        if (!form || form.revision !== request.expectedRevision) throw new Error("Assessment changed; reload and retry.");
        const submitted = command.endsWith("submit_learning_assessment_form");
        form.status = submitted ? "submitted" : "interrupted";
        form.submittedAt = submitted ? Date.now() : null;
        const summary = (state.assessmentWorkspace.forms as Array<Record<string, unknown>>).find((entry) => entry.id === form.id);
        if (summary) Object.assign(summary, { status: form.status, submittedAt: form.submittedAt, gradeStatus: null, score: null });
        return form;
      }
      if (command === "plugin:learning|start_learning_diagnostic") {
        state.planWorkspace.latestDiagnostic = {
          id: "diagnostic-e2e", programId: program.summary.id, status: "active", revision: 0,
          prompts: [{ id: "placement-task", prompt: "Two samples were observed for different periods. Explain how you would make their comparison fair.", outcomeId: "outcome-1", outcomeTitle: "Compare observations", sourceVersionIds: [source.id] }],
          responses: [], findings: [], sourceCoverageGaps: [], interpretation: "Provisional starting-point feedback.", createdAt: Date.now(), submittedAt: null,
        };
        return state.planWorkspace.latestDiagnostic;
      }
      if (command === "plugin:learning|submit_learning_diagnostic") {
        const request = getRequest<Record<string, unknown>>(args);
        const diagnostic = state.planWorkspace.latestDiagnostic;
        if (!diagnostic || diagnostic.id !== request.diagnosticId || diagnostic.revision !== request.expectedDiagnosticRevision) throw new Error("The starting-point answers changed elsewhere. Reload before saving.");
        diagnostic.responses = request.responses;
        diagnostic.revision = Number(diagnostic.revision) + 1;
        if (!request.saveOnly) {
          diagnostic.status = "submitted"; diagnostic.submittedAt = Date.now();
          diagnostic.findings = [{ promptId: "placement-task", outcomeId: "outcome-1", signal: "needs_practice", feedback: "Name a consistent observation period before comparing the measurements.", evidenceQuote: null }];
        }
        return diagnostic;
      }
      if (command === "plugin:learning|skip_learning_diagnostic") {
        state.planWorkspace.latestDiagnostic = { id: "diagnostic-skipped", programId: program.summary.id, status: "skipped", revision: 0, prompts: [], responses: [], findings: [], sourceCoverageGaps: [], interpretation: "Skipped", createdAt: Date.now(), submittedAt: null };
        return state.planWorkspace.latestDiagnostic;
      }
      if (command === "plugin:learning|get_learning_plan") {
        state.planWorkspace.programRevision = program.summary.revision;
        // IPC returns a snapshot, not shared references into the backend's state.
        return structuredClone(state.planWorkspace);
      }
      if (command === "plugin:learning|cancel_learning_generation_job" || command === "plugin:learning|retry_learning_generation_job") {
        const request = getRequest<LearningGenerationJobActionRequestDto>(args);
        if (request.programId !== program.summary.id || request.expectedRevision !== program.summary.revision)
          throw new Error("Program changed; reload and retry.");
        const job = state.planWorkspace.jobs.find((job) => job.id === request.jobId);
        if (!job) throw new Error("Job not found");
        if (command === "plugin:learning|cancel_learning_generation_job") {
          Object.assign(job, { status: "cancelled", finishedAt: Date.now(), progressMessage: "Cancelled", error: null });
          return job;
        }
        const next: LearningGenerationJob = {
          ...job, id: crypto.randomUUID(), operationId: request.operationId, retryOfJobId: job.id,
          status: "pending", createdAt: Date.now(), startedAt: null, finishedAt: null, progressMessage: "Retry queued", error: null,
        };
        state.planWorkspace.jobs.unshift(next);
        return next;
      }
      if (command === "plugin:learning|preview_learning_curriculum_revision") {
        const request = getRequest<Record<string, unknown>>(args);
        if (request.programId !== program.summary.id || request.expectedRevision !== program.summary.revision)
          throw new Error("Program changed; reload and retry.");
        state.planWorkspace.programRevision = program.summary.revision;
        const planState = state.planWorkspace as unknown as Record<string, unknown>;
        const revision = planState.acceptedRevision as Record<string, unknown>;
        const next = JSON.parse(JSON.stringify(revision)) as Record<string, unknown>;
        next.id = "curriculum-revision-e2e-2";
        next.revisionNumber = Number(revision.revisionNumber) + 1;
        next.status = "draft";
        const modules = next.modules as Array<Record<string, unknown>>;
        const operation = (request.operations as Array<Record<string, unknown>>)[0];
        const lessonId = String(operation.lesson_id);
        const lessonRow = (modules[0].lessons as Array<Record<string, unknown>>).find((item) => item.id === lessonId);
        if (!lessonRow) throw new Error("Lesson not found in curriculum.");
        Object.assign(lessonRow, { title: operation.title, objective: operation.objective, estimatedMinutes: operation.estimated_minutes });
        planState.draftRevision = next;
        planState.previewChanges = [{ lessonId, kind: "edit_lesson", description: `Update ${String(operation.title)}.` }];
        planState.requiredLessonCountAfter = planState.requiredLessonCountBefore;
        return state.planWorkspace;
      }
      if (command === "plugin:learning|accept_learning_curriculum_revision") {
        const request = getRequest<Record<string, unknown>>(args);
        if (request.programId !== program.summary.id || request.expectedRevision !== program.summary.revision)
          throw new Error("Program changed; reload and retry.");
        state.planWorkspace.programRevision = program.summary.revision;
        const planState = state.planWorkspace as unknown as Record<string, unknown>;
        const draft = planState.draftRevision as Record<string, unknown> | null;
        if (!draft || draft.id !== request.revisionId) throw new Error("Curriculum revision is no longer available.");
        const revisedModules = draft.modules as Array<{ lessons: Array<{ id: string; title: string; objective: string; estimatedMinutes: number }> }>;
        for (const module of program.modules) {
          for (const lesson of module.lessons) {
            const revised = revisedModules.flatMap((item) => item.lessons).find((item) => item.id === lesson.id);
            if (revised) Object.assign(lesson, { title: revised.title, objective: revised.objective, estimatedMinutes: revised.estimatedMinutes });
          }
        }
        program.summary.revision += 1;
        state.planWorkspace.programRevision = program.summary.revision;
        planState.acceptedRevision = { ...draft, status: "accepted", acceptedAt: Date.now() };
        planState.draftRevision = null;
        planState.previewChanges = [];
        return state.planWorkspace;
      }
      if (command === "plugin:learning|get_learning_practical_draft" || command === "plugin:learning|save_learning_practical_draft") {
        const request = getRequest<LabDraft & { operationId: string; expectedDraftRevision: number }>(args);
        const activity = state.practicalWorkspace.activities.find((item) => item.id === request.activityId);
        if (request.programId !== program.summary.id || !activity || activity.revision !== request.activityRevision)
          throw new Error("The lab activity changed. Reopen it before editing.");
        const key = `${request.programId}:${request.activityId}:${request.activityRevision}`;
        const initial: LabDraft = { programId: request.programId, activityId: request.activityId, activityRevision: request.activityRevision, draftRevision: 0, files: activity.files.filter((file) => file.role === "starter" && file.editable).map(({ path, content }) => ({ path, content })), updatedAt: null };
        const current = state.labDrafts.get(key) ?? initial;
        if (command === "plugin:learning|get_learning_practical_draft") return structuredClone(current);
        state.labDraftSaveCalls.push(structuredClone(request));
        const payload = JSON.stringify(request);
        const previous = state.labDraftOperations.get(request.operationId);
        if (previous) {
          if (previous.payload !== payload) throw new Error("Operation ID was already used with different draft data.");
          return structuredClone(previous.result);
        }
        if (state.labDraftFailuresBeforeSave > 0) {
          state.labDraftFailuresBeforeSave -= 1;
          throw new Error("The draft could not be saved to this device.");
        }
        if (request.expectedDraftRevision !== current.draftRevision) throw new Error("Draft changed; reload before saving.");
        if (!Array.isArray(request.files) || request.files.some((file) => !initial.files.some((starter) => starter.path === file.path)))
          throw new Error("Only editable starter files can be saved.");
        const next = { ...current, draftRevision: current.draftRevision + 1, files: structuredClone(request.files), updatedAt: Date.now() };
        state.labDrafts.set(key, next);
        state.labDraftOperations.set(request.operationId, { payload, result: next });
        persistLabDrafts();
        if (state.labDraftLostResponses > 0) {
          state.labDraftLostResponses -= 1;
          await state.lostSaveResponseGate;
          throw new Error("Draft save response was lost after persistence.");
        }
        return structuredClone(next);
      }
      if (command === "plugin:learning|get_learning_practical_workspace")
        return state.practicalWorkspace;
      if (command === "plugin:learning|get_learning_runtime_catalog") {
        return [
          { id: "csharp", name: "C#", description: "Compile and test a C# project in the .NET SDK.", imageRef: "mcr.microsoft.com/dotnet/sdk:10.0", command: ["dotnet", "run"], entrypointContract: "C# checks project" },
          { id: "rust", name: "Rust", description: "Compile and test Rust exercises.", imageRef: "rust:fixture", command: ["rustc", "--test"], entrypointContract: "Rust checks.rs" },
          { id: "node", name: "React & TypeScript", description: "Type-check React components and run component tests.", imageRef: "lattice-learning/react-ts:fixture", command: ["node", "--test"], entrypointContract: "React checks" },
          { id: "python", name: "Python", description: "Python projects in a container.", imageRef: "python:fixture", command: ["python", "checks.py"], entrypointContract: "Python checks.py" },
        ].map((preset) => ({ ...preset, limits: { timeoutSeconds: 120, memoryMegabytes: 1024, cpuMillis: 1000, processLimit: 64, outputBytes: 262144 } }));
      }
      if (command === "plugin:learning|prepare_learning_runtime_preset") {
        const request = getRequest<{ operationId: string; programId: string; profileId: string; preset: string; engine: "docker" | "podman" }>(args);
        if (!state.practicalWorkspace.runtimeCapabilities.some((capability) => capability.engine === request.engine && capability.available)) {
          throw new Error("Start Docker or Podman before preparing a language environment.");
        }
        if (!state.practicalWorkspace.runtimeProfiles.some((profile) => profile.id === request.profileId)) {
          const names: Record<string, string> = { csharp: "C#", rust: "Rust", node: "React & TypeScript", python: "Python" };
          state.practicalWorkspace.runtimeProfiles.push({
            id: request.profileId, name: names[request.preset], engine: request.engine,
            imageId: `sha256:${"a".repeat(64)}`, command: ["fixture-runner"],
            limits: { timeoutSeconds: 120, memoryMegabytes: 1024, cpuMillis: 1000, processLimit: 64, outputBytes: 262144 },
            enabled: true, revision: 0, createdAt: Date.now(), updatedAt: Date.now(),
          });
        }
        return state.practicalWorkspace;
      }
      if (command === "plugin:learning|get_learning_portability_workspace")
        return state.portabilityWorkspace;
      if (command === "plugin:learning|get_learning_recall_workspace")
        return state.recallV2Workspace;
      if (command === "plugin:learning|save_learning_recall_card") {
        const request = getRequest<Record<string, unknown>>(args);
        const cards = state.recallV2Workspace.cards as Array<Record<string, unknown>>;
        const existing = cards.find((item) => item.id === request.cardId);
        if (existing && existing.contentRevision !== request.expectedContentRevision) throw new Error("Card changed; reload and retry.");
        const content = request.content as Record<string, unknown>;
        if (existing) {
          const versions = existing.versions as Array<Record<string, unknown>>;
          const revision = Number(existing.contentRevision) + 1;
          versions.push({ revision, format: request.format, content: JSON.parse(JSON.stringify(content)), sourceVersionIds: request.sourceVersionIds, changeReason: request.changeReason, createdAt: Date.now() });
          Object.assign(existing, { format: request.format, content: JSON.parse(JSON.stringify(content)), contentRevision: revision, sourceVersionIds: request.sourceVersionIds, updatedAt: Date.now() });
        } else {
          const timestamp = Date.now();
          cards.push({ id: request.cardId, programId: program.summary.id, format: request.format, content: JSON.parse(JSON.stringify(content)), contentRevision: 1, versions: [{ revision: 1, format: request.format, content: JSON.parse(JSON.stringify(content)), sourceVersionIds: request.sourceVersionIds, changeReason: request.changeReason, createdAt: timestamp }], sourceVersionIds: request.sourceVersionIds, scheduler: { schedulerVersion: "expanding_v1", stability: null, difficulty: null, lastReviewedAt: null, dueAt: 0, intervalDays: 0, reviewCount: 0 }, reviewHistory: [], createdAt: timestamp, updatedAt: timestamp });
        }
        return state.recallV2Workspace;
      }
      if (command === "plugin:learning|decide_learning_recall_duplicate") {
        const request = getRequest<Record<string, unknown>>(args);
        const suggestion = (state.recallV2Workspace.duplicates as Array<Record<string, unknown>>).find((item) => item.id === request.suggestionId);
        if (!suggestion || suggestion.status !== "pending") throw new Error("Pending duplicate suggestion not found.");
        suggestion.status = request.accept ? "confirmed" : "dismissed";
        suggestion.decidedAt = Date.now();
        return state.recallV2Workspace;
      }
      if (command === "plugin:learning|change_learning_recall_scheduler") {
        const request = getRequest<Record<string, unknown>>(args);
        const card = (state.recallV2Workspace.cards as Array<Record<string, unknown>>).find((item) => item.id === request.cardId);
        if (!card) throw new Error("Recall card not found.");
        const scheduler = card.scheduler as Record<string, unknown>;
        if (scheduler.reviewCount !== request.expectedReviewCount) throw new Error("Review state changed.");
        scheduler.schedulerVersion = request.schedulerVersion;
        return state.recallV2Workspace;
      }
      if (command === "plugin:learning|review_learning_recall_card") {
        const request = getRequest<Record<string, unknown>>(args);
        const card = (state.recallV2Workspace.cards as Array<Record<string, unknown>>).find((item) => item.id === request.cardId);
        if (!card) throw new Error("Recall card not found.");
        const scheduler = card.scheduler as Record<string, unknown>;
        if (scheduler.reviewCount !== request.expectedReviewCount) throw new Error("Review state changed.");
        const timestamp = Date.now();
        const intervalDays = request.rating === "good" ? 1 : request.rating === "easy" ? 2 : 0;
        Object.assign(scheduler, { reviewCount: Number(scheduler.reviewCount) + 1, intervalDays, dueAt: timestamp + intervalDays * 86_400_000, lastReviewedAt: timestamp });
        (card.reviewHistory as Array<Record<string, unknown>>).push({ id: request.reviewId, rating: request.rating, schedulerVersion: scheduler.schedulerVersion, createdAt: timestamp });
        state.recallV2Workspace.dueCount = (state.recallV2Workspace.cards as Array<Record<string, unknown>>).filter((entry) => ((entry.scheduler as Record<string, unknown>).dueAt as number) <= Date.now()).length;
        return state.recallV2Workspace;
      }
      if (command === "plugin:learning|preview_learning_pack_import") {
        const request = getRequest<Record<string, unknown>>(args);
        state.packMutationCalls.push({ command, request });
        const preview = {
          id: String(request.previewId),
          sourcePath: String(request.sourcePath),
          manifest: {
            format: "lattice-learning",
            version: 1,
            packId: "pack-e2e-1",
            title: "Imported field observations",
            createdAt: Date.now(),
            applicationVersion: "e2e",
            rootSha256: "b".repeat(64),
            privacy: {
              includesPrivateChat: false,
              includesCredentials: false,
              includesAnswerKeys: true,
              includesHiddenEvaluators: false,
              includesLearnerEvidence: true,
              includesPracticalArtifacts: true,
              includesFullSourceBodies: true,
              sourceBodyRedistributionConfirmed: true,
              omittedItems: ["private chat", "hosted runtime secrets"],
            },
            entries: [],
          },
          incomingProgramId: "incoming-program-1",
          incomingProgramTitle: "Imported field observations",
          conflictPolicy: String(request.conflictPolicy),
          conflicts: [],
          changes: [{ entityKind: "program", entityId: "incoming-program-1", action: "create_copy", description: "Create an isolated local program copy." }],
          warnings: [],
          status: "pending",
          canApply: true,
          createdAt: Date.now(),
          decidedAt: null,
        };
        state.portabilityWorkspace.importPreviews = [preview];
        return state.portabilityWorkspace;
      }
      if (command === "plugin:learning|cancel_learning_pack_import_preview") {
        const request = getRequest<Record<string, unknown>>(args);
        state.packMutationCalls.push({ command, request });
        const preview = state.portabilityWorkspace.importPreviews.find((item) => item.id === request.previewId);
        if (preview) { preview.status = "cancelled"; preview.decidedAt = Date.now(); }
        return state.portabilityWorkspace;
      }
      if (command === "plugin:learning|apply_learning_pack_import") {
        const request = getRequest<Record<string, unknown>>(args);
        state.packMutationCalls.push({ command, request });
        const preview = state.portabilityWorkspace.importPreviews.find((item) => item.id === request.previewId);
        if (!preview || preview.manifest.rootSha256 !== request.expectedRootSha256 || !preview.canApply)
          throw new Error("The import preview is no longer eligible.");
        preview.status = "applied";
        preview.decidedAt = Date.now();
        state.portabilityWorkspace.imports.push({ id: "import-e2e-1", previewId: preview.id, importedProgramId: "program-copy-e2e", backupId: null, appliedChanges: preview.changes, importedAt: Date.now() });
        return state.portabilityWorkspace;
      }
      if (command === "plugin:learning|delete_learning_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const item = sourceById(String(request.sourceId));
        if (item.revision !== request.expectedRevision) throw new Error("Source changed; reload and retry.");
        item.deletedAt = Date.now();
        item.deletionReason = String(request.reason);
        item.activeVersionId = null;
        item.activeVersion = null;
        item.revision += 1;
        item.updatedAt = Date.now();
        state.portabilityWorkspace.sourceWorkspace = copySourceWorkspace();
        return copySourceWorkspace();
      }
      if (command === "plugin:learning|reimport_learning_source") {
        const request = getRequest<Record<string, unknown>>(args);
        const item = sourceById(String(request.sourceId));
        if (item.revision !== request.expectedRevision) throw new Error("Source changed; reload and retry.");
        const selected = sourceVersionById(item, String(request.versionId));
        item.activeVersionId = selected.id;
        item.activeVersion = selected;
        item.deletedAt = null;
        item.deletionReason = null;
        item.revision += 1;
        item.updatedAt = Date.now();
        state.portabilityWorkspace.sourceWorkspace = copySourceWorkspace();
        return copySourceWorkspace();
      }
      state.unsupportedCommands.push(command);
      throw new Error(
        `Unsupported Learning Studio fixture command: ${command}`,
      );
    };

    (
      window as unknown as { __LATTICE_TEST_INVOKE__: typeof invoke }
    ).__LATTICE_TEST_INVOKE__ = invoke;
    let nextCallbackId = 1;
    let nextListenerId = 1;
    const callbacks = new Map<number, (...args: unknown[]) => unknown>();
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { label: "main" },
        },
        transformCallback(
          callback: (...args: unknown[]) => unknown,
          once = false,
        ) {
          const id = nextCallbackId++;
          callbacks.set(
            id,
            once
              ? (...args) => {
                  callbacks.delete(id);
                  return callback(...args);
                }
              : callback,
          );
          return id;
        },
        unregisterCallback(id: number) {
          callbacks.delete(id);
        },
        async invoke(
          command: string,
          args?: { event?: string; handler?: number; payload?: unknown },
        ) {
          if (command === "plugin:event|listen") return nextListenerId++;
          if (
            command === "plugin:event|unlisten" ||
            command === "plugin:event|emit"
          )
            return undefined;
          if (command === "plugin:model|check_first_run_status")
            return JSON.stringify({
              needs_setup: false,
              recommended_model_id: null,
              recommended_model_name: null,
              estimated_size_bytes: null,
              });
          if (command === "plugin:dialog|open") return "/Downloads/studio-e2e.lattice-learning";
          return invoke(command, args);
        },
      },
    });
    Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {
      configurable: true,
      value: { unregisterListener: () => undefined },
    });
  }, makeAppSettings());
}
