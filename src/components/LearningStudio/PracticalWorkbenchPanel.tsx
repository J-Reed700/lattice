import { useRef, useState } from "react";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Beaker,
  CheckCircle2,
  CircleHelp,
  Clock3,
  Code2,
  LoaderCircle,
  Play,
  Plus,
  Send,
  Users,
  XCircle,
} from "lucide-react";

import VaultAPI from "@/lib/api";
import type {
  GenerateLearningPracticalActivityRequestDto,
  LearningLabFile,
  LearningPracticalActivityDto,
  LearningPracticalActivityKind,
  LearningPracticalWorkspaceDto,
  LearningSimulationSessionDto,
  LearningPracticalRunStatus,
  StartLearningPracticalRunRequestDto,
  StartLearningSimulationRequestDto,
  SendLearningSimulationTurnRequestDto,
  FinishLearningSimulationRequestDto,
  LearningLessonDto,
  LearningProgramDto,
} from "@/lib/bindings";

import { ActivityComposer, modeCopy } from "./ActivityComposer";
import { LearningCodeEditor } from "./LearningCodeEditor";
import { RuntimeSetupPanel } from "./RuntimeSetupPanel";
import { useLearningLabDraft } from "./useLearningLabDraft";

const workspaceKey = (programId: string) =>
  ["learning-practical-workspace", programId] as const;
function uuid() {
  return (
    globalThis.crypto?.randomUUID?.() ?? "00000000-0000-4000-8000-000000000003"
  );
}
function unwrap<T>(
  result: { ok: true; data: T } | { ok: false; error: string },
): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
function errorText(error: unknown) {
  return error instanceof Error
    ? error.message
    : "The request could not be completed.";
}
function dateText(value?: number | null) {
  if (value == null) return "Time unavailable";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "Time unavailable"
    : date.toLocaleString();
}
function isSimulationKind(kind: LearningPracticalActivityKind) {
  return ["incident", "system_design", "interview", "conversation"].includes(
    kind,
  );
}

function usePracticalWorkspace(programId: string) {
  return useQuery<LearningPracticalWorkspaceDto>({
    queryKey: workspaceKey(programId),
    queryFn: async () =>
      unwrap(await VaultAPI.getLearningPracticalWorkspace(programId)),
    enabled: Boolean(programId),
    staleTime: 2_000,
    retry: false,
    refetchInterval: (query) =>
      query.state.data?.runs.some(
        (run) => run.status === "pending" || run.status === "running",
      )
        ? 1_000
        : false,
  });
}

function statusStyle(status: LearningPracticalRunStatus) {
  if (status === "passed") return "text-emerald-700 dark:text-emerald-300 bg-emerald-500/10";
  if (status === "failed") return "text-rose-700 dark:text-rose-300 bg-rose-500/10";
  if (status === "running" || status === "pending")
    return "text-accent bg-accent/10";
  return "text-text-muted bg-background";
}

function SimulationPanel({
  programId,
  activity,
  session,
  onSession,
}: {
  programId: string;
  activity: LearningPracticalActivityDto;
  session: LearningSimulationSessionDto | null;
  onSession: (session: LearningSimulationSessionDto) => void;
}) {
  const [learnerRole, setLearnerRole] = useState("Incident lead");
  const [counterpartRole, setCounterpartRole] = useState("On-call engineer");
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const retryRef = useRef<{
    start?: StartLearningSimulationRequestDto;
    turn?: SendLearningSimulationTurnRequestDto;
    finish?: FinishLearningSimulationRequestDto;
  }>({});
  const start = useMutation({
    retry: false,
    mutationFn: async (request: StartLearningSimulationRequestDto) =>
      unwrap(await VaultAPI.startLearningSimulation(request)),
    onSuccess: onSession,
  });
  const turn = useMutation({
    retry: false,
    mutationFn: async (request: SendLearningSimulationTurnRequestDto) =>
      unwrap(await VaultAPI.sendLearningSimulationTurn(request)),
    onSuccess: onSession,
  });
  const finish = useMutation({
    retry: false,
    mutationFn: async (request: FinishLearningSimulationRequestDto) =>
      unwrap(await VaultAPI.finishLearningSimulation(request)),
    onSuccess: onSession,
  });
  const begin = async () => {
    if (!activity) return;
    setError(null);
    const fingerprint = `${activity.id}:${learnerRole.trim()}:${counterpartRole.trim()}`;
    let request = retryRef.current.start;
    if (
      !request ||
      `${request.activityId}:${request.learnerRole}:${request.counterpartRole}` !==
        fingerprint
    ) {
      request = {
        operationId: uuid(),
        sessionId: uuid(),
        programId,
        activityId: activity.id,
        expectedActivityRevision: activity.revision,
        practiceSessionId: null,
        learnerRole: learnerRole.trim(),
        counterpartRole: counterpartRole.trim(),
      };
      retryRef.current.start = request;
    }
    try {
      const created = await start.mutateAsync(request);
      retryRef.current.start = undefined;
      onSession(created);
    } catch (cause) {
      setError(errorText(cause));
    }
  };
  const send = async () => {
    if (!session || !message.trim()) return;
    const content = message.trim();
    let request = retryRef.current.turn;
    if (
      request?.sessionId !== session.id ||
      request.expectedRevision !== session.revision ||
      request.content !== content
    ) {
      request = {
        operationId: uuid(),
        programId,
        sessionId: session.id,
        expectedRevision: session.revision,
        content,
      };
      retryRef.current.turn = request;
    }
    try {
      const updated = await turn.mutateAsync(request);
      retryRef.current.turn = undefined;
      onSession(updated);
      setMessage("");
      setError(null);
    } catch (cause) {
      setError(errorText(cause));
    }
  };
  const end = async () => {
    if (!session) return;
    let request = retryRef.current.finish;
    if (
      request?.sessionId !== session.id ||
      request.expectedRevision !== session.revision
    ) {
      request = {
        operationId: uuid(),
        programId,
        sessionId: session.id,
        expectedRevision: session.revision,
      };
      retryRef.current.finish = request;
    }
    try {
      onSession(await finish.mutateAsync(request));
      retryRef.current.finish = undefined;
    } catch (cause) {
      setError(errorText(cause));
    }
  };
  return (
    <section className="rounded-2xl border border-border bg-surface p-4 sm:p-5">
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
          <Users size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">
            Role-based simulation
          </div>
          <h3 className="mt-1 font-serif text-xl text-text-primary">
            Take a role, respond to the situation
          </h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            The counterpart responds in turns. This is a practice record tied to
            activity revision {activity.revision}; it does not claim operational
            readiness.
          </p>
        </div>
      </div>
      {!session ? (
        <div className="mt-4 grid gap-3 sm:grid-cols-2">
          <label className="text-xs font-medium text-text-secondary">
            Your role
            <input
              value={learnerRole}
              onChange={(event) => setLearnerRole(event.target.value)}
              maxLength={120}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm"
            />
          </label>
          <label className="text-xs font-medium text-text-secondary">
            Counterpart role
            <input
              value={counterpartRole}
              onChange={(event) => setCounterpartRole(event.target.value)}
              maxLength={120}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm"
            />
          </label>
          <button
            type="button"
            disabled={
              start.isPending || !learnerRole.trim() || !counterpartRole.trim()
            }
            onClick={() => void begin()}
            className="inline-flex w-fit items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45"
          >
            <Play size={13} /> Start simulation
          </button>
        </div>
      ) : (
        <>
          <div className="mt-4 flex flex-wrap items-center justify-between gap-3 rounded-xl bg-background px-4 py-3">
            <div className="text-xs text-text-secondary">
              <strong className="text-text-primary">
                {session.learnerRole}
              </strong>{" "}
              with{" "}
              <strong className="text-text-primary">
                {session.counterpartRole}
              </strong>{" "}
              · {session.status}
            </div>
            {session.status === "active" && (
              <button
                type="button"
                disabled={finish.isPending}
                onClick={() => void end()}
                className="rounded-full border border-border px-3 py-2 text-xs"
              >
                Finish simulation
              </button>
            )}
          </div>
          <div
            aria-label="Simulation turns"
            className="mt-3 max-h-[420px] space-y-3 overflow-y-auto rounded-xl border border-border bg-background/40 p-3"
          >
            {session.turns.length === 0 ? (
              <p className="p-4 text-xs text-text-muted">
                Your first response will begin the conversation.
              </p>
            ) : (
              session.turns.map((turnItem) => (
                <article
                  key={turnItem.id}
                  className={`max-w-[92%] rounded-xl border p-3 ${turnItem.speaker === "learner" ? "ml-auto border-accent/20 bg-accent/5" : "border-border bg-surface"}`}
                >
                  <div className="text-xs font-semibold uppercase tracking-[.12em] text-text-muted">
                    {turnItem.speaker === "learner"
                      ? session.learnerRole
                      : turnItem.speaker === "counterpart"
                        ? session.counterpartRole
                        : "Coach"}
                  </div>
                  <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-text-secondary">
                    {turnItem.content}
                  </p>
                  {turnItem.citations.map((citation, index) => (
                    <blockquote
                      key={`${citation.sourceId}:${index}`}
                      className="mt-2 border-l-2 border-accent/40 pl-2 text-xs italic text-text-muted"
                    >
                      “{citation.quote}”
                    </blockquote>
                  ))}
                </article>
              ))
            )}
          </div>
          {session.status === "active" && (
            <div className="mt-3 flex gap-2">
              <label className="sr-only" htmlFor="simulation-message">
                Your response
              </label>
              <textarea
                id="simulation-message"
                rows={3}
                maxLength={4000}
                value={message}
                onChange={(event) => setMessage(event.target.value)}
                onKeyDown={(event) => {
                  if ((event.metaKey || event.ctrlKey) && event.key === "Enter")
                    void send();
                }}
                placeholder={`Respond as ${session.learnerRole}…`}
                className="min-w-0 flex-1 resize-y rounded-xl border border-border bg-background px-3 py-2.5 text-sm leading-6 outline-none focus:border-accent"
              />
              <button
                type="button"
                aria-label="Send simulation turn"
                disabled={turn.isPending || !message.trim()}
                onClick={() => void send()}
                className="self-end rounded-full bg-accent p-3 text-accent-fg disabled:opacity-40"
              >
                <Send size={15} />
              </button>
            </div>
          )}
        </>
      )}
      {error && (
        <p
          role="alert"
          className="mt-3 rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700 dark:text-rose-300"
        >
          {error}{" "}
          <button
            type="button"
            onClick={() =>
              retryRef.current.finish
                ? void end()
                : session
                  ? void send()
                  : void begin()
            }
            className="ml-2 underline"
          >
            Retry same operation
          </button>
        </p>
      )}
    </section>
  );
}

export function PracticalWorkbenchPanel({
  program,
  lesson,
}: {
  program: LearningProgramDto;
  lesson: LearningLessonDto | null;
}) {
  const programId = program.summary.id;
  const query = usePracticalWorkspace(programId);
  const client = useQueryClient();
  const generate = useMutation({
    retry: false,
    mutationFn: async (request: GenerateLearningPracticalActivityRequestDto) =>
      unwrap(await VaultAPI.generateLearningPracticalActivity(request)),
    onSuccess: (data) => client.setQueryData(workspaceKey(programId), data),
  });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [composerOpen, setComposerOpen] = useState(false);
  const newActivityRef = useRef<HTMLButtonElement>(null);
  const [runtimeSetupRequest, setRuntimeSetupRequest] = useState(0);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [startingRun, setStartingRun] = useState(false);
  const startingRunRef = useRef(false);
  const [simulation, setSimulation] =
    useState<LearningSimulationSessionDto | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const runRequestRef = useRef<{
    fingerprint: string;
    request: StartLearningPracticalRunRequestDto;
  } | null>(null);
  const cancelRequestRef = useRef<{
    operationId: string;
    programId: string;
    runId: string;
  } | null>(null);
  const workspace = query.data;
  const activities =
    workspace?.activities.filter(
      (item) => !lesson || item.lessonId === lesson.id,
    ) ?? [];
  const activeActivity =
    activities.find((item) => item.id === selectedId) ?? activities[0];
  const capabilities = workspace?.runtimeCapabilities ?? [];
  const activeRun = workspace?.runs.find(
    (run) =>
      run.activityId === activeActivity?.id &&
      (run.status === "running" || run.status === "pending"),
  );
  const runs =
    workspace?.runs
      .filter((run) => run.activityId === activeActivity?.id)
      .sort((a, b) => b.createdAt - a.createdAt) ?? [];
  const profiles = workspace?.runtimeProfiles ?? [];
  const builtinRuntimes = workspace?.builtinRuntimes ?? [];
  const runMutation = useMutation({
    retry: false,
    mutationFn: async (request: StartLearningPracticalRunRequestDto) =>
      unwrap(await VaultAPI.startLearningPracticalRun(request)),
    onSuccess: async () => {
      await query.refetch();
    },
  });
  const cancelRun = useMutation({
    retry: false,
    mutationFn: async (request: {
      operationId: string;
      programId: string;
      runId: string;
    }) => unwrap(await VaultAPI.cancelLearningPracticalRun(request)),
    onSuccess: async () => {
      cancelRequestRef.current = null;
      await query.refetch();
    },
  });
  const selectedRun =
    runs.find((item) => item.id === activeRun?.id) ??
    runs.find((item) => item.id === selectedRunId) ??
    runs[0];
  const profile = profiles.find(
    (item) => item.id === activeActivity?.runtimeProfileId,
  );
  const capability = capabilities.find(
    (item) => item.engine === profile?.engine,
  );
  const builtinRuntime = builtinRuntimes.find(
    (item) => item.id === activeActivity?.builtinRuntime,
  );
  const canRunActivity =
    activeActivity?.runtimeKind === "container"
      ? Boolean(
          activeActivity.runtimeAvailable &&
          profile?.enabled &&
          capability?.available,
        )
      : activeActivity?.runtimeKind === "builtin"
        ? Boolean(activeActivity.runtimeAvailable && builtinRuntime?.available)
        : false;

  const draft = useLearningLabDraft({
    programId,
    activityId: activeActivity?.id ?? null,
    activityRevision: activeActivity?.revision ?? 0,
    starterFiles: activeActivity?.files
      .filter((file) => file.role === "starter" && file.editable)
      .map((file) => ({ path: file.path, content: file.content })) ?? [],
  });
  const editorFiles = draft.files;
  const selectedFile = editorFiles.find((file) => file.path === selectedFilePath) ?? editorFiles[0];

  if (query.isLoading)
    return (
      <div
        role="status"
        className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted"
      >
        <LoaderCircle className="mr-2 inline animate-spin" size={16} />
        Loading practical workspace…
      </div>
    );
  if (query.isError || !workspace)
    return (
      <section className="rounded-2xl border border-rose-500/20 bg-surface p-6">
        <h3 className="font-serif text-xl text-text-primary">
          Labs &amp; simulations unavailable
        </h3>
        <p role="alert" className="mt-2 text-sm text-text-secondary">
          {errorText(query.error)}
        </p>
        <button
          type="button"
          onClick={() => void query.refetch()}
          className="mt-4 rounded-full border border-border px-4 py-2 text-sm"
        >
          Retry
        </button>
      </section>
    );

  const beginRun = async () => {
    if (!activeActivity || !canRunActivity || startingRunRef.current || activeRun || draft.loadState !== "ready") return;
    startingRunRef.current = true;
    setStartingRun(true);
    setRunError(null);
    const filesAtStart: LearningLabFile[] = editorFiles.map((file) => ({ ...file }));
    try {
      if (!await draft.flush()) return;
      const fingerprint = JSON.stringify({
        activityId: activeActivity.id,
        revision: activeActivity.revision,
        editorFiles: filesAtStart,
      });
      let request =
        runRequestRef.current?.fingerprint === fingerprint
          ? runRequestRef.current.request
          : null;
      if (!request) {
        request = {
          operationId: uuid(),
          runId: uuid(),
          programId,
          activityId: activeActivity.id,
          expectedActivityRevision: activeActivity.revision,
          practiceSessionId: null,
          learnerFiles: filesAtStart,
        };
        runRequestRef.current = { fingerprint, request };
      }
      await runMutation.mutateAsync(request);
      runRequestRef.current = null;
    } catch (cause) {
      setRunError(errorText(cause));
    } finally {
      startingRunRef.current = false;
      setStartingRun(false);
    }
  };
  const output = selectedRun;

  return (
    <div className="space-y-5" data-testid="practical-workbench-panel">
      <header>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.16em] text-accent">
              <Beaker size={14} /> Labs &amp; simulations
            </div>
            <h2 className="mt-1 font-serif text-2xl text-text-primary">
              Work through a real brief
            </h2>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">
              Choose a task, work through the brief, and learn from the results.
            </p>
          </div>
          <button
            ref={newActivityRef}
            type="button"
            disabled={lesson?.preparation !== "ready" || generate.isPending}
            onClick={() => setComposerOpen(true)}
            className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45"
          >
            <Plus size={14} /> New activity
          </button>
        </div>
      </header>

      <RuntimeSetupPanel
        programId={programId}
        workspace={workspace}
        openRequest={runtimeSetupRequest}
      />

      <div className="grid min-w-0 grid-cols-1 gap-5 xl:grid-cols-[220px_minmax(0,1fr)]">
        <aside className="min-w-0 space-y-4">
          <section className="rounded-2xl border border-border bg-surface p-4">
            <div className="flex items-center justify-between gap-2">
              <div>
                <div className="text-xs font-semibold uppercase tracking-[.15em] text-text-muted">
                  Activity shelf
                </div>
                <h3 className="mt-1 font-serif text-xl text-text-primary">
                  {lesson?.title ?? "Choose a lesson"}
                </h3>
              </div>
              <span className="rounded-full bg-background px-2.5 py-1 text-xs text-text-muted">
                {activities.length}
              </span>
            </div>
            {lesson?.preparation !== "ready" ? (
              <div className="mt-4 rounded-xl bg-amber-500/10 p-3 text-xs leading-5 text-text-secondary">
                Prepare the selected lesson before creating a practical
                activity.
              </div>
            ) : null}
            {activities.length === 0 ? (
              <div className="mt-4 rounded-xl border border-dashed border-border bg-background p-5 text-center">
                <Code2 className="mx-auto h-6 w-6 text-text-muted" />
                <p className="mt-2 text-xs leading-5 text-text-muted">
                  No activities for this lesson yet.
                </p>
                <button
                  type="button"
                  disabled={lesson?.preparation !== "ready" || generate.isPending}
                  onClick={() => setComposerOpen(true)}
                  className="mt-3 text-xs font-semibold text-accent disabled:opacity-40"
                >
                  Create first activity
                </button>
              </div>
            ) : (
              <div className="mt-4 space-y-2">
                {activities.map((activity) => (
                  <button
                    type="button"
                    key={activity.id}
                    aria-pressed={activeActivity?.id === activity.id}
                    onClick={async () => {
                      if (!await draft.flush()) return;
                      setSelectedId(activity.id);
                      setSelectedFilePath(null);
                      setSelectedRunId(null);
                      setSimulation(null);
                    }}
                    className={`w-full rounded-xl border p-3 text-left transition ${activeActivity?.id === activity.id ? "border-accent/50 bg-accent/5" : "border-border bg-background/40 hover:border-accent/30"}`}
                  >
                    <span className="flex items-center justify-between gap-2">
                      <span className="truncate text-xs font-semibold uppercase tracking-[.12em] text-accent">
                        {activity.kind.replace("_", " ")}
                      </span>
                      <span className="text-xs capitalize text-text-muted">
                        {activity.status}
                      </span>
                    </span>
                    <span className="mt-1 block font-serif text-base leading-5 text-text-primary">
                      {activity.title}
                    </span>
                    <span className="mt-2 line-clamp-2 block text-xs leading-4 text-text-secondary">
                      {activity.brief}
                    </span>
                    <span className="mt-2 block text-xs text-text-muted">
                      Revision {activity.revision} ·{" "}
                      {activity.runtimeKind === "none"
                        ? "No runtime needed"
                        : activity.runtimeAvailable
                          ? activity.runtimeKind === "builtin"
                            ? "Included environment ready"
                            : "Container environment ready"
                          : "Runtime unavailable"}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </section>
        </aside>

        <main className="min-w-0 space-y-4">
          {!activeActivity ? (
            <section className="grid min-h-64 place-items-center rounded-2xl border border-dashed border-border bg-surface p-7 text-center">
              <div>
                <CircleHelp className="mx-auto h-7 w-7 text-text-muted" />
                <h3 className="mt-3 font-serif text-xl text-text-primary">
                  Your workspace starts with an activity
                </h3>
                <p className="mt-2 max-w-sm text-sm leading-6 text-text-secondary">
                  Create a brief from a ready lesson to open its artifact editor
                  or role simulation.
                </p>
              </div>
            </section>
          ) : (
            <>
              <section className="rounded-2xl border border-border bg-surface p-4 sm:p-6">
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="rounded-full bg-accent/10 px-2.5 py-1 text-xs font-semibold uppercase tracking-wider text-accent">
                        {activeActivity.kind.replace("_", " ")}
                      </span>
                      <span className="text-xs text-text-muted">
                        Revision {activeActivity.revision} · generated with{" "}
                        {activeActivity.generatorModel}
                      </span>
                    </div>
                    <h3 className="mt-2 font-serif text-2xl text-text-primary">
                      {activeActivity.title}
                    </h3>
                    <p className="mt-3 whitespace-pre-wrap text-sm leading-7 text-text-secondary">
                      {activeActivity.brief}
                    </p>
                  </div>
                  <span
                    className={`shrink-0 rounded-full px-3 py-1.5 text-xs font-medium ${activeActivity.runtimeAvailable ? "bg-emerald-500/10 text-emerald-800 dark:text-emerald-200" : "bg-amber-500/10 text-amber-900 dark:text-amber-100"}`}
                  >
                    {activeActivity.runtimeKind === "none" ? "Review activity" : activeActivity.runtimeAvailable
                      ? "Ready to run"
                      : "Execution unavailable"}
                  </span>
                </div>
                <details className="mt-4 border-t border-border pt-3">
                  <summary className="cursor-pointer text-sm font-medium text-text-secondary">Success criteria &amp; allowed help</summary>
                  <div className="mt-3 grid gap-3 sm:grid-cols-2">
                  <div className="rounded-xl border border-border bg-background/45 p-4">
                    <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
                      Help available · {activeActivity.practiceMode}
                    </div>
                    <p className="mt-2 text-xs leading-5 text-text-secondary">
                      {modeCopy[activeActivity.practiceMode]}
                    </p>
                    <div className="mt-2 flex flex-wrap gap-1.5">
                      {activeActivity.allowedAids.length ? (
                        activeActivity.allowedAids.map((aid) => (
                          <span
                            key={aid}
                            className="rounded-full bg-surface px-2.5 py-1 text-xs text-text-muted"
                          >
                            {aid}
                          </span>
                        ))
                      ) : (
                        <span className="rounded-full bg-surface px-2.5 py-1 text-xs text-text-muted">
                          No aids recorded
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="rounded-xl border border-border bg-background/45 p-4">
                    <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
                      What to notice
                    </div>
                    <ul className="mt-2 space-y-1 text-xs leading-5 text-text-secondary">
                      {activeActivity.rubric.map((criterion) => (
                        <li key={criterion.id}>
                          <span className="font-medium text-text-primary">
                            {criterion.title}
                          </span>{" "}
                          · {criterion.description}
                        </li>
                      ))}
                    </ul>
                  </div>
                  </div>
                </details>
              </section>

              {activeActivity.files.length > 0 && (
                <section className="rounded-2xl border border-border bg-surface p-4 sm:p-6">
                  <div className="flex flex-wrap items-end justify-between gap-3">
                    <div>
                      <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">
                        Artifact editor
                      </div>
                      <h3 className="mt-1 font-serif text-xl text-text-primary">
                        Change the starter work
                      </h3>
                    </div>
                      <button
                        type="button"
                        disabled={
                          !canRunActivity ||
                          draft.loadState !== "ready" ||
                          startingRun ||
                          !editorFiles.length ||
                          runMutation.isPending ||
                          Boolean(activeRun)
                        }
                        onClick={() => void beginRun()}
                        className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-40"
                      >
                        {startingRun || runMutation.isPending || activeRun ? (
                          <LoaderCircle size={13} className="animate-spin" />
                        ) : (
                          <Play size={13} />
                        )}
                        {startingRun || runMutation.isPending
                          ? "Starting run…"
                          : activeRun
                            ? "Run in progress…"
                            : "Run locally"}
                      </button>
                    <span role="status" aria-live="polite" className="text-xs text-text-muted">
                      {draft.loadState === "loading" ? "Restoring your work…" : draft.saveState === "saving" ? "Saving…" : draft.saveState === "dirty" ? "Unsaved changes" : draft.error ? "Your changes need attention" : "Saved on this device"}
                    </span>
                  </div>
                  {draft.error && <div role="alert" className="mt-3 rounded-xl border border-rose-500/25 bg-rose-500/5 p-3 text-sm text-text-primary">
                    <p>{draft.error}</p>
                    <div className="mt-2 flex flex-wrap gap-3">
                      {draft.saveState === "conflict" ? <>
                        <button type="button" onClick={() => void draft.keepLocalEdits()} className="font-medium text-accent underline underline-offset-4">Keep my edits</button>
                        <button type="button" onClick={() => void draft.useSavedVersion()} className="text-text-secondary underline underline-offset-4">Use saved version</button>
                        <p className="w-full text-xs text-text-muted">Keep my edits saves your changed files over the latest draft. Use saved version replaces your unsaved changes.</p>
                      </> : <button type="button" onClick={() => void (draft.loadState === "error" ? draft.retryLoad() : draft.flush())} className="font-medium text-accent underline underline-offset-4">{draft.loadState === "error" ? "Retry loading draft" : "Retry saving draft"}</button>}
                    </div>
                  </div>}
                  <div className="mt-4 overflow-hidden rounded-xl border border-border">
                    {editorFiles.length > 0 && <div role="tablist" aria-label="Activity files" className="flex flex-wrap gap-1 border-b border-border bg-background p-2">
                      {editorFiles.map((file, index) => <button key={file.path} type="button" role="tab" id={`lab-file-tab-${activeActivity.id}-${index}`} aria-controls={`lab-file-panel-${activeActivity.id}-${index}`} aria-selected={file.path === selectedFile?.path} tabIndex={file.path === selectedFile?.path ? 0 : -1}
                        onClick={() => setSelectedFilePath(file.path)}
                        onKeyDown={(event) => {
                          const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
                          if (!step && event.key !== "Home" && event.key !== "End") return;
                          event.preventDefault();
                          const target = event.key === "Home" ? 0 : event.key === "End" ? editorFiles.length - 1 : (index + step + editorFiles.length) % editorFiles.length;
                          setSelectedFilePath(editorFiles[target].path);
                          document.getElementById(`lab-file-tab-${activeActivity.id}-${target}`)?.focus();
                        }}
                        className={`max-w-full break-all rounded-lg px-3 py-2 font-mono text-xs transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent ${file.path === selectedFile?.path ? "bg-surface text-accent shadow-sm" : "text-text-secondary hover:bg-surface"}`}>{file.path}</button>)}
                    </div>}
                    {editorFiles.map((file, index) => <div key={`${activeActivity.id}:${activeActivity.revision}:${file.path}`} id={`lab-file-panel-${activeActivity.id}-${index}`} role="tabpanel" aria-labelledby={`lab-file-tab-${activeActivity.id}-${index}`} hidden={selectedFile?.path !== file.path}>
                      <LearningCodeEditor path={file.path} showPath={false} value={file.content} ariaLabel={`Edit ${file.path}`} readOnly={draft.loadState !== "ready"} onChange={(content) => draft.setFileContent(file.path, content)} onRunShortcut={() => void beginRun()} />
                    </div>)}
                    {editorFiles.length === 0 && <p className="bg-background p-4 text-sm text-text-muted">This activity has no editable starter files.</p>}
                  </div>
                  {activeActivity.runtimeKind !== "none" && !canRunActivity && (
                    <div className="mt-4 flex items-start gap-3 rounded-xl border border-amber-500/25 bg-amber-500/5 p-4">
                      <AlertTriangle
                        size={16}
                        className="mt-0.5 shrink-0 text-amber-700"
                      />
                      <div>
                        <h4 className="text-xs font-semibold text-text-primary">
                          This device cannot run this activity right now
                        </h4>
                        <p className="mt-1 text-xs leading-5 text-text-secondary">
                          {activeActivity.runtimeUnavailableReason ||
                            builtinRuntime?.reason ||
                            capability?.reason ||
                            "A matching execution environment is not available."}{" "}
                          Your edits are saved here while you set up the required language environment.
                        </p>
                      </div>
                    </div>
                  )}
                  {activeActivity.runtimeKind !== "none" && (
                    <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
                      <p className="text-xs leading-4 text-text-muted">
                        {activeActivity.runtimeKind === "builtin"
                          ? "This activity runs in the built-in language environment."
                          : "Runs on this device in the prepared language environment."}
                      </p>

                    </div>
                  )}
                </section>
              )}

              {runError && (
                <p
                  role="alert"
                  className="rounded-xl bg-rose-500/10 p-4 text-sm text-rose-700 dark:text-rose-300"
                >
                  {runError}{" "}
                  <button
                    type="button"
                    onClick={() => void beginRun()}
                    className="ml-2 underline"
                  >
                    Retry run
                  </button>
                </p>
              )}
              {output && (
                <section
                  aria-label="Run output"
                  className="rounded-2xl border border-border bg-[#211f1b] p-4 text-[#f3eee6] sm:p-5"
                >
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <span
                        className={`rounded-full px-2.5 py-1 text-xs font-semibold capitalize ${statusStyle(output.status)}`}
                      >
                        {output.status === "interrupted"
                          ? "Interrupted · partial output"
                          : output.status.replace("_", " ")}
                      </span>
                      <span className="text-xs text-[#b8b0a6]">
                        {dateText(output.createdAt)} · activity revision{" "}
                        {output.activityRevision}
                      </span>
                    </div>
                    {output.status === "running" && (
                      <button
                        type="button"
                        disabled={cancelRun.isPending}
                        onClick={() => {
                          const request =
                            cancelRequestRef.current?.runId === output.id
                              ? cancelRequestRef.current
                              : {
                                  operationId: uuid(),
                                  programId,
                                  runId: output.id,
                                };
                          cancelRequestRef.current = request;
                          cancelRun.mutate(request);
                        }}
                        className="rounded-full border border-white/20 px-3 py-1.5 text-xs"
                      >
                        {cancelRun.isPending ? "Cancelling…" : "Cancel run"}
                      </button>
                    )}
                  </div>
                  <div className="mt-4 grid gap-3 lg:grid-cols-2">
                    <div>
                      <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-[#b8b0a6]">
                        Output
                      </h4>
                      <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-xl border border-white/10 bg-black/25 p-3 font-mono text-[11px] leading-5">
                        {output.stdout ||
                          (output.status === "running"
                            ? "Waiting for runtime output…"
                            : "No standard output.")}
                      </pre>
                    </div>
                    <div>
                      <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-[#b8b0a6]">
                        Errors
                      </h4>
                      <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-xl border border-white/10 bg-black/25 p-3 font-mono text-[11px] leading-5">
                        {output.stderr || "No error output."}
                      </pre>
                    </div>
                  </div>
                  {output.outputTruncated && (
                    <p className="mt-2 text-xs text-amber-200">
                      Output was truncated by the configured runtime limit.
                    </p>
                  )}
                  {output.checks.length > 0 && (
                    <div className="mt-4">
                      <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-[#b8b0a6]">
                        Checks
                      </h4>
                      <div className="mt-2 grid gap-2 sm:grid-cols-2">
                        {output.checks.map((check) => (
                          <div
                            key={check.name}
                            className="rounded-lg border border-white/10 bg-white/5 p-3"
                          >
                            <div className="flex items-center gap-2 text-xs">
                              <span>
                                {check.status === "passed" ? (
                                  <CheckCircle2
                                    size={13}
                                    className="text-emerald-300"
                                  />
                                ) : check.status === "failed" ||
                                  check.status === "error" ? (
                                  <XCircle
                                    size={13}
                                    className="text-rose-300"
                                  />
                                ) : (
                                  <Clock3
                                    size={13}
                                    className="text-[#b8b0a6]"
                                  />
                                )}
                              </span>
                              <span>{check.name}</span>
                              <span className="ml-auto capitalize text-xs text-[#b8b0a6]">
                                {check.status}
                              </span>
                            </div>
                            <p className="mt-1 text-xs leading-4 text-[#c8c0b5]">
                              {check.message}
                            </p>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </section>
              )}
              {cancelRun.error && (
                <p
                  role="alert"
                  className="rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700 dark:text-rose-300"
                >
                  {errorText(cancelRun.error)}{" "}
                  <button
                    type="button"
                    onClick={() => {
                      const request = cancelRequestRef.current;
                      if (request) cancelRun.mutate(request);
                    }}
                    className="ml-2 underline"
                  >
                    Retry cancel
                  </button>
                </p>
              )}

              {isSimulationKind(activeActivity.kind) && (
                <SimulationPanel
                  programId={programId}
                  activity={activeActivity}
                  session={
                    simulation ??
                    workspace.simulations.find(
                      (entry) =>
                        entry.activityId === activeActivity.id &&
                        entry.status === "active",
                    ) ??
                    null
                  }
                  onSession={(session) => {
                    setSimulation(session);
                    void query.refetch();
                  }}
                />
              )}
              <section className="rounded-2xl border border-border bg-surface p-4 sm:p-5">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
                      Run history
                    </div>
                    <h3 className="mt-1 font-serif text-lg text-text-primary">
                      This activity’s attempts
                    </h3>
                  </div>
                  <span className="rounded-full bg-background px-3 py-1.5 text-xs text-text-muted">
                    {runs.length}
                  </span>
                </div>
                {runs.length === 0 ? (
                  <p className="mt-3 rounded-xl bg-background p-4 text-xs text-text-muted">
                    Runs appear here after a local runtime starts. Conversation
                    turns are kept separately.
                  </p>
                ) : (
                  <div className="mt-3 divide-y divide-border">
                    {runs.map((run) => (
                      <button
                        type="button"
                        key={run.id}
                        aria-pressed={selectedRun?.id === run.id}
                        onClick={() => setSelectedRunId(run.id)}
                        className={`flex w-full flex-wrap items-center justify-between gap-2 py-3 text-left first:pt-0 ${selectedRun?.id === run.id ? "text-accent" : ""}`}
                      >
                        <span className="text-xs text-text-secondary">
                          {dateText(run.createdAt)} · revision{" "}
                          {run.activityRevision}
                        </span>
                        <span
                          className={`rounded-full px-2.5 py-1 text-xs capitalize ${statusStyle(run.status)}`}
                        >
                          {run.status}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </section>
            </>
          )}
        </main>
      </div>
      {composerOpen && (
        <ActivityComposer
          program={program}
          lesson={lesson}
          profiles={profiles}
          builtinRuntimes={builtinRuntimes}
          returnFocusRef={newActivityRef}
          onClose={() => setComposerOpen(false)}
          onRequestRuntimeSetup={() => {
            setComposerOpen(false);
            setRuntimeSetupRequest((request) => request + 1);
          }}
          onCreated={async (request) => {
            if (!await draft.flush()) {
              throw new Error("Save the current activity's edits before creating another activity. Close this dialog to retry saving.");
            }
            await generate.mutateAsync(request);
            setSelectedId(request.activityId);
            setSelectedRunId(null);
          }}
        />
      )}
    </div>
  );
}
