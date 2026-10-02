import type { ReactNode } from "react";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LearningLessonDto, LearningProgramDto } from "@/lib/bindings";

import { PracticalWorkbenchPanel } from "./PracticalWorkbenchPanel";

const mocks = vi.hoisted(() => ({
  workspace: vi.fn(),
  runtimeCatalog: vi.fn(),
  prepareRuntime: vi.fn(),
  generate: vi.fn(),
  startRun: vi.fn(),
  cancelRun: vi.fn(),
  startSimulation: vi.fn(),
  turn: vi.fn(),
  finish: vi.fn(),
  getDraft: vi.fn(),
  saveDraft: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  default: {
    getLearningPracticalWorkspace: mocks.workspace,
    getLearningRuntimeCatalog: mocks.runtimeCatalog,
    prepareLearningRuntimePreset: mocks.prepareRuntime,
    generateLearningPracticalActivity: mocks.generate,
    startLearningPracticalRun: mocks.startRun,
    cancelLearningPracticalRun: mocks.cancelRun,
    startLearningSimulation: mocks.startSimulation,
    sendLearningSimulationTurn: mocks.turn,
    finishLearningSimulation: mocks.finish,
    getLearningPracticalDraft: mocks.getDraft,
    saveLearningPracticalDraft: mocks.saveDraft,
  },
}));

// Test the workspace's persistence and execution contract here. The real
// CodeMirror view has its own tests and is exercised by browser journeys.
vi.mock("./LearningCodeEditor", () => ({
  LearningCodeEditor: ({ value, onChange, readOnly, ariaLabel }: { value: string; onChange: (value: string) => void; readOnly?: boolean; ariaLabel?: string }) =>
    <textarea aria-label={ariaLabel} value={value} readOnly={readOnly} onChange={(event) => onChange(event.target.value)} />,
}));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const program = {
  summary: { id: "program-1", title: "Reason from evidence", revision: 4 },
} as LearningProgramDto;
const lesson = {
  id: "lesson-1",
  title: "Inspect the evidence",
  preparation: "ready",
  objective: "Separate observations from inference.",
} as LearningLessonDto;
const profile = {
  id: "profile-1",
  name: "Python sandbox",
  engine: "docker",
  imageId: "sha256:fixed",
  command: ["python", "/work/main.py"],
  limits: {
    timeoutSeconds: 10,
    memoryMegabytes: 256,
    cpuMillis: 500,
    processLimit: 32,
    outputBytes: 8192,
  },
  enabled: true,
  revision: 1,
  createdAt: 1_790_000_000_000,
  updatedAt: 1_790_000_000_000,
};
const file = {
  path: "main.py",
  role: "starter",
  content: 'print("hello")',
  contentSha256: "starter-sha",
  editable: true,
};
const activity = (overrides: Record<string, unknown> = {}) => ({
  id: "activity-1",
  programId: "program-1",
  lessonId: "lesson-1",
  predecessorId: null,
  kind: "code_lab",
  title: "Inspect a data trail",
  brief: "Read the captured data and print a bounded summary.",
  status: "ready",
  practiceMode: "practice",
  allowedAids: ["Saved sources"],
  sourceVersionIds: ["version-1"],
  rubric: [
    {
      id: "criterion-1",
      title: "Use evidence",
      description: "Point to the relevant observation.",
      maxPoints: 3,
    },
  ],
  runtimeKind: "container",
  runtimeProfileId: "profile-1",
  runtimeAvailable: true,
  runtimeUnavailableReason: null,
  generatorModel: "Learning model",
  files: [file],
  revision: 2,
  createdAt: 1_790_000_000_000,
  updatedAt: 1_790_000_000_000,
  ...overrides,
});
const run = (overrides: Record<string, unknown> = {}) => ({
  id: "run-1",
  programId: "program-1",
  activityId: "activity-1",
  activityRevision: 2,
  practiceSessionId: null,
  status: "passed",
  engine: "docker",
  imageId: "sha256:fixed",
  learnerFiles: [{ path: "main.py", content: 'print("edited")' }],
  stdout: "hello from the local runtime",
  stderr: "",
  outputTruncated: false,
  exitCode: 0,
  durationMs: 27,
  checks: [
    {
      name: "Output contains summary",
      status: "passed",
      message: "The requested summary was printed.",
      durationMs: 2,
    },
  ],
  createdAt: 1_790_100_000_000,
  completedAt: 1_790_100_000_100,
  ...overrides,
});
const builtinRuntimes = [
  {
    id: "javascript",
    name: "JavaScript",
    description: "Standard ECMAScript modules.",
    available: true,
    reason: null,
  },
  {
    id: "python",
    name: "Python",
    description: "Python with its standard library.",
    available: true,
    reason: null,
  },
];
const workspace = (overrides: Record<string, unknown> = {}) => ({
  programId: "program-1",
  runtimeCapabilities: [
    { engine: "docker", available: true, version: "25.0", reason: null },
  ],
  builtinRuntimes,
  runtimeProfiles: [profile],
  activities: [activity()],
  runs: [],
  simulations: [],
  ...overrides,
});
const session = (overrides: Record<string, unknown> = {}) => ({
  id: "session-1",
  programId: "program-1",
  activityId: "activity-1",
  activityRevision: 2,
  practiceSessionId: null,
  learnerRole: "Incident lead",
  counterpartRole: "On-call engineer",
  status: "active",
  revision: 1,
  turns: [],
  createdAt: 1_790_000_000_000,
  updatedAt: 1_790_000_000_000,
  submittedAt: null,
  ...overrides,
});

function renderPractical(chosenLesson = lesson) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(
    <PracticalWorkbenchPanel program={program} lesson={chosenLesson} />,
    { wrapper },
  );
}

describe("Learning Studio Labs & simulations", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.workspace.mockResolvedValue(ok(workspace()));
    const drafts = new Map<string, { programId: string; activityId: string; activityRevision: number; draftRevision: number; files: Array<{ path: string; content: string }>; updatedAt: number | null }>();
    mocks.getDraft.mockImplementation(async (request) => ok(drafts.get(request.activityId) ?? { ...request, draftRevision: 0, files: [{ path: file.path, content: file.content }], updatedAt: null }));
    mocks.saveDraft.mockImplementation(async (request) => {
      const saved = { programId: request.programId, activityId: request.activityId, activityRevision: request.activityRevision, draftRevision: request.expectedDraftRevision + 1, files: request.files, updatedAt: Date.now() };
      drafts.set(request.activityId, saved);
      return ok(saved);
    });
    mocks.runtimeCatalog.mockResolvedValue(ok([]));
    mocks.prepareRuntime.mockResolvedValue(ok(workspace()));
    mocks.generate.mockResolvedValue(ok(workspace()));
    mocks.startRun.mockResolvedValue(ok(run()));
    mocks.cancelRun.mockResolvedValue(ok(run({ status: "cancelled" })));
    mocks.startSimulation.mockResolvedValue(ok(session()));
    mocks.turn.mockImplementation(async (request) =>
      ok(
        session({
          revision: request.expectedRevision + 1,
          turns: [
            {
              id: "turn-1",
              ordinal: 1,
              speaker: "learner",
              content: request.content,
              citations: [],
              modelName: null,
              createdAt: 1_790_000_000_000,
            },
            {
              id: "turn-2",
              ordinal: 2,
              speaker: "counterpart",
              content: "What observation led you to that conclusion?",
              citations: [
                {
                  sourceId: "source-1",
                  versionId: "version-1",
                  quote: "A saved source quote.",
                },
              ],
              modelName: "Simulation model",
              createdAt: 1_790_000_000_001,
            },
          ],
        }),
      ),
    );
    mocks.finish.mockResolvedValue(
      ok(session({ status: "submitted", revision: 2 })),
    );
  });

  it("keeps execution disabled and explains a missing runtime capability", async () => {
    mocks.workspace.mockResolvedValue(
      ok(
        workspace({
          runtimeCapabilities: [
            {
              engine: "docker",
              available: false,
              version: null,
              reason: "Docker is not installed.",
            },
          ],
          activities: [
            activity({
              runtimeAvailable: false,
              runtimeUnavailableReason: "No compatible local runtime.",
            }),
          ],
        }),
      ),
    );
    renderPractical();
    expect(
      await screen.findByText("This device cannot run this activity right now"),
    ).toBeVisible();
    expect(screen.getByText(/No compatible local runtime/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Run locally" })).toBeDisabled();
    expect(
      screen.queryByText("hello from the local runtime"),
    ).not.toBeInTheDocument();
  });

  it("runs the learner-edited file, renders real output/checks and records the activity revision", async () => {
    const user = userEvent.setup();
    let current = workspace();
    mocks.workspace.mockImplementation(async () => ok(current));
    mocks.startRun.mockImplementation(async (request) => {
      expect(request.learnerFiles).toEqual([
        { path: "main.py", content: 'print("edited")' },
      ]);
      current = workspace({ runs: [run()] });
      return ok(run());
    });
    renderPractical();
    const editor = await screen.findByRole("textbox", { name: "Edit main.py" });
    await user.clear(editor);
    await user.type(editor, 'print("edited")');
    await user.click(screen.getByRole("button", { name: "Run locally" }));
    expect(
      await screen.findByText("hello from the local runtime"),
    ).toBeVisible();
    expect(screen.getByText("Output contains summary")).toBeVisible();
    expect(screen.getByText(/activity revision 2/)).toBeVisible();
    expect(mocks.startRun.mock.calls[0][0]).toMatchObject({
      activityId: "activity-1",
      expectedActivityRevision: 2,
      learnerFiles: [{ path: "main.py" }],
    });
  });

  it("reuses the exact run operation after a lost response and labels interruption honestly", async () => {
    const user = userEvent.setup();
    let current = workspace();
    mocks.workspace.mockImplementation(async () => ok(current));
    mocks.startRun
      .mockImplementationOnce(async () => fail("Connection interrupted"))
      .mockImplementationOnce(async () => {
        current = workspace({
          runs: [run({ status: "interrupted", stdout: "partial output" })],
        });
        return ok(run({ status: "interrupted", stdout: "partial output" }));
      });
    renderPractical();
    await screen.findByRole("textbox", { name: "Edit main.py" });
    await user.click(screen.getByRole("button", { name: "Run locally" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Connection interrupted",
    );
    const first = mocks.startRun.mock.calls.at(-1)?.[0];
    await user.click(screen.getByRole("button", { name: "Retry run" }));
    await waitFor(() => expect(mocks.startRun).toHaveBeenCalledTimes(2));
    expect(mocks.startRun.mock.calls[1][0]).toEqual(first);
    expect(
      await screen.findByText("Interrupted · partial output"),
    ).toBeVisible();
  });

  it("starts a role-based simulation and retries a turn with the same operation and revision", async () => {
    const user = userEvent.setup();
    mocks.workspace.mockResolvedValue(
      ok(workspace({ activities: [activity({ kind: "interview" })] })),
    );
    mocks.startSimulation
      .mockResolvedValueOnce(fail("Connection interrupted"))
      .mockResolvedValueOnce(ok(session()));
    renderPractical();
    await user.click(await screen.findByRole("button", { name: /interview/ }));
    await user.click(screen.getByRole("button", { name: "Start simulation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Connection interrupted",
    );
    const startRequest = mocks.startSimulation.mock.calls[0][0];
    await user.click(
      screen.getByRole("button", { name: "Retry same operation" }),
    );
    await waitFor(() => expect(mocks.startSimulation).toHaveBeenCalledTimes(2));
    expect(mocks.startSimulation.mock.calls[1][0]).toEqual(startRequest);
    const response = screen.getByRole("textbox", { name: "Your response" });
    await user.type(response, "I would first confirm the timeline.");
    await user.click(
      screen.getByRole("button", { name: "Send simulation turn" }),
    );
    expect(
      await screen.findByText("What observation led you to that conclusion?"),
    ).toBeVisible();
    expect(screen.getByText(/A saved source quote/)).toBeVisible();
    expect(mocks.turn.mock.calls[0][0]).toMatchObject({
      expectedRevision: 1,
      content: "I would first confirm the timeline.",
    });
  });

  it("does not offer activity generation until the selected lesson is ready", async () => {
    renderPractical({ ...lesson, preparation: "outline" } as LearningLessonDto);
    expect(
      await screen.findByText(
        "Prepare the selected lesson before creating a practical activity.",
      ),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "New activity" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "Generate activity" }),
    ).not.toBeInTheDocument();
  });

  it("offers included language environments and sends the selected runtime exclusively", async () => {
    const user = userEvent.setup();
    renderPractical();

    await user.click(
      await screen.findByRole("button", { name: "New activity" }),
    );
    await user.selectOptions(
      screen.getByLabelText("Execution environment"),
      "builtin:javascript",
    );
    await user.type(
      screen.getByLabelText("Your focus for this activity"),
      "Parse and summarize a small data file.",
    );
    await user.click(screen.getByRole("button", { name: "Generate activity" }));

    await waitFor(() => expect(mocks.generate).toHaveBeenCalledTimes(1));
    expect(mocks.generate.mock.calls[0][0]).toMatchObject({
      builtinRuntime: "javascript",
      runtimeProfileId: null,
      kind: "code_lab",
    });
  });

  it("keeps container selection available for optional project activities", async () => {
    const user = userEvent.setup();
    renderPractical();

    await user.click(
      await screen.findByRole("button", { name: "New activity" }),
    );
    await user.selectOptions(screen.getByLabelText("Activity type"), "project");
    await user.selectOptions(
      screen.getByLabelText("Execution environment"),
      "profile-1",
    );
    await user.type(
      screen.getByLabelText("Your focus for this activity"),
      "Build and test a small CLI.",
    );
    await user.click(screen.getByRole("button", { name: "Generate activity" }));

    await waitFor(() => expect(mocks.generate).toHaveBeenCalledTimes(1));
    expect(mocks.generate.mock.calls[0][0]).toMatchObject({
      kind: "project",
      runtimeProfileId: "profile-1",
      builtinRuntime: null,
    });
  });

  it("shows a runtime setup path when neither included nor container environments are ready", async () => {
    const user = userEvent.setup();
    mocks.workspace.mockResolvedValue(
      ok(workspace({ builtinRuntimes: [], runtimeProfiles: [] })),
    );
    renderPractical();

    await user.click(
      await screen.findByRole("button", { name: "New activity" }),
    );
    expect(
      screen.getByText(/No execution environments are ready yet/),
    ).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Set up an environment" }),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Execution environments/ }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("guards duplicate activity submissions and retries with the same operation ID", async () => {
    const user = userEvent.setup();
    mocks.generate
      .mockResolvedValueOnce(fail("Connection interrupted"))
      .mockResolvedValueOnce(ok(workspace()));
    renderPractical();

    await user.click(
      await screen.findByRole("button", { name: "New activity" }),
    );
    await user.type(
      screen.getByLabelText("Your focus for this activity"),
      "Trace the input validation path.",
    );
    await user.selectOptions(screen.getByLabelText("Execution environment"), "builtin:python");
    await user.click(screen.getByRole("button", { name: "Generate activity" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Connection interrupted",
    );
    const originalRequest = mocks.generate.mock.calls[0][0];

    await user.click(screen.getByRole("button", { name: "Retry generation" }));
    await waitFor(() => expect(mocks.generate).toHaveBeenCalledTimes(2));
    expect(mocks.generate.mock.calls[1][0]).toEqual(originalRequest);
  });

  it("focuses the activity dialog, closes with Escape, and restores the opener", async () => {
    const user = userEvent.setup();
    renderPractical();
    const opener = await screen.findByRole("button", { name: "New activity" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Practice in context" });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    await user.tab({ shift: true });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("requires a runnable language for code labs while allowing an explicit review project", async () => {
    const user = userEvent.setup();
    renderPractical();
    await user.click(await screen.findByRole("button", { name: "New activity" }));
    await user.type(screen.getByLabelText("Your focus for this activity"), "Practice a small example.");
    expect(screen.getByRole("button", { name: "Generate activity" })).toBeDisabled();
    await user.selectOptions(screen.getByLabelText("Execution environment"), "builtin:python");
    expect(screen.getByRole("button", { name: "Generate activity" })).toBeEnabled();
    await user.selectOptions(screen.getByLabelText("Activity type"), "project");
    await user.selectOptions(screen.getByLabelText("Execution environment"), "");
    await user.click(screen.getByRole("button", { name: "Generate activity" }));
    await waitFor(() => expect(mocks.generate).toHaveBeenCalledWith(expect.objectContaining({ kind: "project", runtimeProfileId: null, builtinRuntime: null })));
  });

  it("restores a draft after leaving the lab without running it", async () => {
    const user = userEvent.setup();
    const first = renderPractical();
    const editor = await screen.findByRole("textbox", { name: "Edit main.py" });
    await waitFor(() => expect(editor).not.toHaveAttribute("readonly"));
    await user.clear(editor);
    await user.type(editor, 'print("saved before running")');
    first.unmount();
    await waitFor(() => expect(mocks.saveDraft).toHaveBeenCalled());
    renderPractical();
    expect(await screen.findByRole("textbox", { name: "Edit main.py" })).toHaveValue('print("saved before running")');
    expect(mocks.startRun).not.toHaveBeenCalled();
  });

  it("keeps an edited activity open if its draft cannot be saved before switching", async () => {
    const user = userEvent.setup();
    mocks.workspace.mockResolvedValue(ok(workspace({ activities: [activity(), activity({ id: "activity-2", title: "Another activity" })] })));
    mocks.saveDraft.mockResolvedValue(fail("Disk full"));
    renderPractical();
    const editor = await screen.findByRole("textbox", { name: "Edit main.py" });
    await waitFor(() => expect(editor).not.toHaveAttribute("readonly"));
    await user.clear(editor);
    await user.type(editor, "important work");
    await user.click(screen.getByRole("button", { name: /Another activity/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Disk full");
    expect(editor).toHaveValue("important work");
    expect(screen.getByRole("button", { name: /Another activity/ })).toHaveAttribute("aria-pressed", "false");
    expect(within(screen.getByRole("alert")).getByRole("button", { name: "Retry saving draft" })).toBeVisible();
  });
});
