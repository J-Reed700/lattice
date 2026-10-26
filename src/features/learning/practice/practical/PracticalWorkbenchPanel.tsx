import { useEffect, useRef, useState } from "react";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Beaker, CircleHelp, LoaderCircle, Plus } from "lucide-react";

import { useApi } from '@/contexts/ApiContext';
import { invalidateLearningSourceDependents } from "@/features/learning/api/learningInvalidation";
import { learningPracticalWorkspaceKey as workspaceKey } from "@/features/learning/api/learningQueryKeys";
import { ActivityBrief } from "@/features/learning/practice/practical/ActivityBrief";
import { ActivityComposer } from "@/features/learning/practice/practical/ActivityComposer";
import { ActivityShelf } from "@/features/learning/practice/practical/ActivityShelf";
import { ArtifactEditor } from "@/features/learning/practice/practical/ArtifactEditor";
import {
  errorText,
  isSimulationKind,
  unwrap,
  uuid,
} from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import { RunHistory } from "@/features/learning/practice/practical/RunHistory";
import { RunOutput } from "@/features/learning/practice/practical/RunOutput";
import { RuntimeSetupPanel } from "@/features/learning/practice/practical/RuntimeSetupPanel";
import { SimulationPanel } from "@/features/learning/practice/practical/SimulationPanel";
import { useLearningLabDraft } from "@/features/learning/practice/practical/useLearningLabDraft";
import { usePracticalWorkspace } from "@/features/learning/practice/practical/usePracticalWorkspace";
import type {
  GenerateLearningPracticalActivityRequestDto,
  LearningLabFile,
  LearningSimulationSessionDto,
  StartLearningPracticalRunRequestDto,
  LearningLessonDto,
  LearningProgramDto,
} from "@/lib/bindings";

export function PracticalWorkbenchPanel({
  program,
  lesson,
  projectRequest,
}: {
  program: LearningProgramDto;
  lesson: LearningLessonDto | null;
  projectRequest?: { id: string; brief: string } | null;
}) {
  const api = useApi();
  const programId = program.summary.id;
  const query = usePracticalWorkspace(programId);
  const client = useQueryClient();
  const generate = useMutation({
    retry: false,
    mutationFn: async (request: GenerateLearningPracticalActivityRequestDto) =>
      unwrap(await api.generateLearningPracticalActivity(request)),
    onSuccess: (data) => {
      client.setQueryData(workspaceKey(programId), data);
    },
    onSettled: (_data, _error, request) => {
      // Research can save useful evidence even when a later model call fails.
      if (request.kind === "interview") {
        void invalidateLearningSourceDependents(client, programId);
        void client.invalidateQueries({
          queryKey: ["learning-sources", programId],
        });
      }
    },
  });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [composerOpen, setComposerOpen] = useState(false);
  const [projectBrief, setProjectBrief] = useState<string | null>(null);
  useEffect(() => {
    if (projectRequest) { setProjectBrief(projectRequest.brief); setComposerOpen(true); }
  }, [projectRequest]);
  const newActivityRef = useRef<HTMLButtonElement>(null);
  const [runtimeSetupRequest, setRuntimeSetupRequest] = useState(0);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [startingRun, setStartingRun] = useState(false);
  const startingRunRef = useRef(false);
  const [simulation, setSimulation] =
    useState<LearningSimulationSessionDto | { newAttemptFor: string } | null>(null);
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
  const savedSimulation = workspace?.simulations.find(
    (entry) => entry.activityId === activeActivity?.id && entry.status === "active",
  ) ?? workspace?.simulations.find((entry) => entry.activityId === activeActivity?.id) ?? null;
  const latestSelectedSimulation = simulation && !("newAttemptFor" in simulation)
    ? workspace?.simulations.find((entry) => entry.id === simulation.id && entry.revision > simulation.revision) ?? simulation
    : null;
  const activeSimulation = simulation && "newAttemptFor" in simulation
    ? simulation.newAttemptFor === activeActivity?.id ? null : savedSimulation
    : latestSelectedSimulation?.activityId === activeActivity?.id ? latestSelectedSimulation : savedSimulation;
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
      unwrap(await api.startLearningPracticalRun(request)),
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
    }) => unwrap(await api.cancelLearningPracticalRun(request)),
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

      {/* The dialog can close while an activity is prepared; say what is happening, and what failed. */}
      {!composerOpen && generate.isPending && (
        <p role="status" className="flex items-center gap-2 rounded-xl border border-border bg-surface px-4 py-3 text-sm text-text-secondary">
          <LoaderCircle size={15} className="shrink-0 animate-spin text-accent" />
          {generate.variables?.kind === "interview"
            ? "Preparing your interview… Company research and question planning can take several minutes."
            : "Preparing your activity…"}
        </p>
      )}
      {!composerOpen && generate.isError && (
        <p role="alert" className="rounded-xl bg-rose-500/10 px-4 py-3 text-sm text-rose-700 dark:text-rose-300">
          The activity could not be prepared: {errorText(generate.error)}
        </p>
      )}

      <RuntimeSetupPanel
        programId={programId}
        workspace={workspace}
        openRequest={runtimeSetupRequest}
      />

      <div className="grid min-w-0 grid-cols-1 gap-5 xl:grid-cols-[220px_minmax(0,1fr)]">
        <ActivityShelf
          lesson={lesson}
          activities={activities}
          activeActivity={activeActivity}
          generatePending={generate.isPending}
          onCreate={() => setComposerOpen(true)}
          onSelectActivity={async (activity) => {
            if (!await draft.flush()) return;
            setSelectedId(activity.id);
            setSelectedFilePath(null);
            setSelectedRunId(null);
            setSimulation(null);
          }}
        />

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
              <ActivityBrief activeActivity={activeActivity} />

              {activeActivity.files.length > 0 && (
                <ArtifactEditor
                  activeActivity={activeActivity}
                  draft={draft}
                  editorFiles={editorFiles}
                  selectedFile={selectedFile}
                  setSelectedFilePath={setSelectedFilePath}
                  canRunActivity={canRunActivity}
                  startingRun={startingRun}
                  runPending={runMutation.isPending}
                  activeRun={activeRun}
                  beginRun={beginRun}
                  builtinRuntime={builtinRuntime}
                  capability={capability}
                />
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
                <RunOutput
                  output={output}
                  cancelling={cancelRun.isPending}
                  onCancel={() => {
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
                />
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
                  key={activeActivity.id}
                  programId={programId}
                  activity={activeActivity}
                  session={activeSimulation}
                  onRestart={() => setSimulation({ newAttemptFor: activeActivity.id })}
                  onSession={(session) => {
                    setSimulation(session);
                    void query.refetch();
                  }}
                />
              )}
              <RunHistory
                runs={runs}
                selectedRun={selectedRun}
                setSelectedRunId={setSelectedRunId}
              />
            </>
          )}
        </main>
      </div>
      {composerOpen && (
        <ActivityComposer
          program={program}
          lesson={lesson}
          initialBrief={projectBrief ?? undefined}
          initialKind={projectBrief ? "project" : undefined}
          profiles={profiles}
          builtinRuntimes={builtinRuntimes}
          returnFocusRef={newActivityRef}
          onClose={() => {
            setComposerOpen(false);
            setProjectBrief(null);
            // An error the dialog already showed need not linger below it.
            if (!generate.isPending) generate.reset();
          }}
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
            setSimulation(null);
          }}
        />
      )}
    </div>
  );
}
