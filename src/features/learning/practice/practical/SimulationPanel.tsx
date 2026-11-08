import React, { useMemo, useRef, useState, useEffect } from "react";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Play, Users } from "lucide-react";

import { TiptapViewer } from "@/components/TiptapEditor";
import { useApi } from "@/contexts/ApiContext";
import { learningPracticalWorkspaceKey } from "@/features/learning/api/learningQueryKeys";
import { LEARNER_TEXT_MAX_CHARS } from "@/features/learning/model/learnerTextLimit";
import { AnswerTimer } from "@/features/learning/practice/practical/AnswerTimer";
import {
  errorText,
  questionLabel,
  unwrap,
  uuid,
} from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import { PracticeCitation } from "@/features/learning/practice/PracticeCitation";
import {
  PracticeThread,
  type PracticeThreadTurn,
} from "@/features/learning/practice/shared/PracticeThread";
import { StatusLine } from "@/features/learning/practice/shared/StatusLine";
import { useCitationDisplayStore } from "@/features/reading/stores/citationDisplayStore";
import type {
  FinishLearningSimulationRequestDto,
  LearningPracticalActivityDto,
  LearningSimulationSessionDto,
  SendLearningSimulationTurnRequestDto,
  StartLearningSimulationRequestDto,
} from "@/lib/bindings";
import { useDraft } from "@/shared/drafts/drafts";

export function SimulationPanel({
  programId,
  activity,
  session,
  onSession,
  onRestart,
}: {
  programId: string;
  activity: LearningPracticalActivityDto;
  session: LearningSimulationSessionDto | null;
  onSession: (session: LearningSimulationSessionDto) => void;
  onRestart?: () => void;
}): JSX.Element {
  const api = useApi();
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const client = useQueryClient();
  const isInterview = activity.kind === "interview" || Boolean(activity.interviewPlan);

  // Computed default roles based on activity
  const defaultLearnerRole =
    activity.interviewPlan?.learnerRole ||
    (activity.kind === "interview" ? "Candidate" : "Incident lead");
  const defaultCounterpartRole =
    activity.interviewPlan?.counterpartRole ||
    (activity.kind === "interview" ? "Interviewer" : "On-call engineer");

  // Replace effect-derived role state with render-time computed values
  const [customLearnerRole, setCustomLearnerRole] = useState<string | null>(null);
  const [customCounterpartRole, setCustomCounterpartRole] = useState<string | null>(null);
  const [prevActivityKey, setPrevActivityKey] = useState(activity.id);

  if (prevActivityKey !== activity.id) {
    setPrevActivityKey(activity.id);
    setCustomLearnerRole(null);
    setCustomCounterpartRole(null);
  }

  const learnerRole = customLearnerRole ?? defaultLearnerRole;
  const counterpartRole = customCounterpartRole ?? defaultCounterpartRole;

  // Kept per session, so leaving the simulation does not lose the reply.
  const draftKey = session ? `simulation:${session.id}` : null;
  const [message, setMessage] = useDraft(draftKey);
  const [error, setError] = useState<string | null>(null);
  const [retryOperation, setRetryOperation] = useState<"start" | "turn" | "finish" | null>(null);
  const busyRef = useRef(false);
  const logRef = useRef<HTMLDivElement>(null);
  const retryRef = useRef<{
    start?: StartLearningSimulationRequestDto;
    turn?: SendLearningSimulationTurnRequestDto;
    finish?: FinishLearningSimulationRequestDto;
  }>({});

  const refreshAfterError = () => {
    void client.invalidateQueries({ queryKey: learningPracticalWorkspaceKey(programId) });
  };

  // Bring the newest turn into view inside the log: the interviewer's next
  // question, or the start of the closing feedback, not the end of it.
  const turnCount = session?.turns.length ?? 0;
  useEffect(() => {
    const log = logRef.current;
    const newest = log?.lastElementChild;
    if (!log || !newest || turnCount === 0) return;
    log.scrollTop += newest.getBoundingClientRect().top - log.getBoundingClientRect().top - 12;
  }, [session?.id, turnCount]);

  const start = useMutation({
    retry: false,
    mutationFn: async (request: StartLearningSimulationRequestDto) =>
      unwrap(await api.startLearningSimulation(request)),
    onError: refreshAfterError,
  });

  const turn = useMutation({
    retry: false,
    mutationFn: async (request: SendLearningSimulationTurnRequestDto) =>
      unwrap(await api.sendLearningSimulationTurn(request)),
    onError: refreshAfterError,
  });

  const finish = useMutation({
    retry: false,
    mutationFn: async (request: FinishLearningSimulationRequestDto) =>
      unwrap(await api.finishLearningSimulation(request)),
    onError: refreshAfterError,
  });

  const busy = start.isPending || turn.isPending || finish.isPending;

  const busyLabel = useMemo(() => {
    if (start.isPending) {
      return isInterview ? "Starting the interview…" : "Starting simulation…";
    }
    if (finish.isPending) {
      return "Preparing your feedback…";
    }
    if (turn.isPending) {
      return isInterview ? "The interviewer is responding…" : "Your counterpart is responding…";
    }
    return null;
  }, [start.isPending, finish.isPending, turn.isPending, isInterview]);

  const lastQuestionId = session?.turns.filter((item) => item.speaker === "counterpart").at(-1)?.id;

  const begin = async () => {
    if (busyRef.current || !learnerRole.trim() || !counterpartRole.trim()) return;
    busyRef.current = true;
    setError(null);
    const fingerprint = `${activity.id}:${learnerRole.trim()}:${counterpartRole.trim()}`;
    let request = retryRef.current.start;
    if (
      !request ||
      `${request.activityId}:${request.learnerRole}:${request.counterpartRole}` !== fingerprint
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
      setRetryOperation(null);
      onSession(created);
    } catch (cause) {
      setRetryOperation("start");
      setError(errorText(cause));
    } finally {
      busyRef.current = false;
    }
  };

  const send = async (retryPending = false, explicitContent?: string) => {
    if (
      busyRef.current ||
      session?.status !== "active" ||
      (!explicitContent && !message.trim() && !(retryPending && retryRef.current.turn))
    ) {
      return;
    }
    busyRef.current = true;
    setError(null);
    const content = (explicitContent ?? message).trim();
    let request = retryRef.current.turn;
    if (
      request?.sessionId !== session.id ||
      (!retryPending && (request.expectedRevision !== session.revision || request.content !== content))
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
      if (message.trim() === request.content) setMessage("");
      setError(null);
      setRetryOperation(null);
    } catch (cause) {
      setRetryOperation("turn");
      setError(errorText(cause));
    } finally {
      busyRef.current = false;
    }
  };

  const end = async () => {
    if (
      busyRef.current ||
      session?.status !== "active" ||
      !session.turns.some((item) => item.speaker === "learner")
    ) {
      return;
    }
    busyRef.current = true;
    setError(null);
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
      setRetryOperation(null);
    } catch (cause) {
      setRetryOperation("finish");
      setError(errorText(cause));
    } finally {
      busyRef.current = false;
    }
  };

  const threadTurns: PracticeThreadTurn[] = useMemo(() => {
    if (!session) return [];
    return session.turns.map((turnItem) => ({
      id: turnItem.id,
      speaker: turnItem.speaker === "learner" ? "learner" : turnItem.speaker === "counterpart" ? "counterpart" : "coach",
      speakerLabel:
        turnItem.speaker === "learner"
          ? session.learnerRole
          : turnItem.speaker === "counterpart"
            ? session.counterpartRole
            : "Coach",
      content: turnItem.content,
      extra: showCitations && turnItem.citations && turnItem.citations.length > 0 ? (
        <div className="mt-2 space-y-1">
          {turnItem.citations.map((citation, index) => (
            <PracticeCitation
              key={`${citation.sourceId}:${index}`}
              programId={programId}
              citation={citation}
              canOpen={activity.practiceMode !== "demonstrate" || session.status === "submitted"}
            />
          ))}
        </div>
      ) : null,
    }));
  }, [session, showCitations, programId, activity.practiceMode]);

  return (
    <section className="rounded-2xl border border-border-default bg-surface p-4 sm:p-5">
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent-muted text-accent">
          <Users size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">
            {isInterview ? "Company interview practice" : "Role-based simulation"}
          </div>
          <h3 className="mt-1 font-serif text-xl text-text-primary">
            {isInterview ? "Answer one question at a time" : "Take a role, respond to the situation"}
          </h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            {isInterview ? (
              "The interviewer opens the conversation and probes your reasoning. Finish the simulation to review feedback against the role’s criteria."
            ) : (
              <>
                The counterpart responds in turns. This is a practice record tied to activity revision{" "}
                {activity.revision}; it does not claim operational readiness.
              </>
            )}
          </p>
        </div>
      </div>

      {!session ? (
        <div className="mt-4 grid gap-3 sm:grid-cols-2">
          <label className="text-xs font-medium text-text-secondary">
            Your role
            <input
              value={learnerRole}
              onChange={(event) => setCustomLearnerRole(event.target.value)}
              maxLength={100}
              disabled={busy}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2.5 text-sm text-text-primary"
            />
          </label>
          <label className="text-xs font-medium text-text-secondary">
            Counterpart role
            <input
              value={counterpartRole}
              onChange={(event) => setCustomCounterpartRole(event.target.value)}
              maxLength={100}
              disabled={busy}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2.5 text-sm text-text-primary"
            />
          </label>
          <div className="sm:col-span-2">
            <button
              type="button"
              disabled={busy || !learnerRole.trim() || !counterpartRole.trim()}
              onClick={() => void begin()}
              className="inline-flex w-fit items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45"
            >
              <Play size={13} /> {isInterview ? "Start interview" : "Start simulation"}
            </button>
          </div>
          {busy && <StatusLine busy={busyLabel} />}
          {error && (
            <StatusLine
              error={error}
              onRetry={() => void begin()}
              retryAriaLabel="Retry same operation"
            />
          )}
        </div>
      ) : (
        <div className="mt-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl bg-surface-sunken px-4 py-3">
            <div className="text-xs text-text-secondary">
              <strong className="text-text-primary">{session.learnerRole}</strong> with{" "}
              <strong className="text-text-primary">{session.counterpartRole}</strong> ·{" "}
              {session.status === "active" ? "in progress" : "finished"}
            </div>
            {session.status === "active" && (
              <button
                type="button"
                disabled={busy || !session.turns.some((item) => item.speaker === "learner")}
                onClick={() => void end()}
                className="rounded-full border border-border-default bg-surface px-3 py-2 text-xs font-medium text-text-primary hover:border-accent hover:text-accent disabled:opacity-40"
              >
                {isInterview ? "Finish interview" : "Finish simulation"}
              </button>
            )}
            {session.status !== "active" && onRestart && (
              <button
                type="button"
                onClick={onRestart}
                className="rounded-full border border-border-default bg-surface px-3 py-2 text-xs font-medium text-text-primary hover:border-accent hover:text-accent"
              >
                Start another attempt
              </button>
            )}
          </div>

          {session.status === "active" ? (
            <PracticeThread
              conversationId={session.id}
              turns={threadTurns}
              draftKey={draftKey}
              placeholder={`Respond as ${session.learnerRole}…`}
              sendLabel="Send"
              sendAriaLabel="Send simulation turn"
              busyLabel={busyLabel}
              error={error}
              onRetry={() => {
                if (retryOperation === "finish") void end();
                else if (retryOperation === "turn") void send(true);
                else void begin();
              }}
              retryAriaLabel="Retry same operation"
              disabled={busy}
              maxLength={LEARNER_TEXT_MAX_CHARS}
              logRef={logRef}
              logLabel="Simulation turns"
              inputLabel="Your response"
              emptyText={
                activity.interviewPlan
                  ? "Introduce yourself to begin the interview."
                  : "Your first response will begin the conversation."
              }
              clearOnSend={false}
              onSend={async (content) => {
                await send(false, content);
              }}
              renderTurnContent={(turnItem) =>
                turnItem.speaker === "learner" ? (
                  <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-text-secondary">
                    {turnItem.content}
                  </p>
                ) : (
                  <div className="mt-1 text-sm leading-6 text-text-secondary">
                    <TiptapViewer content={turnItem.content} images={isInterview ? [] : undefined} />
                  </div>
                )
              }
              composerTop={
                <>
                  {isInterview && lastQuestionId && !finish.isPending && (
                    <AnswerTimer since={lastQuestionId} running={!turn.isPending} />
                  )}
                  {activity.practiceMode !== "demonstrate" &&
                    activity.interviewPlan?.questions &&
                    activity.interviewPlan.questions.length > 0 && (
                      <div
                        className="flex flex-wrap items-center gap-1.5 pt-1"
                        role="group"
                        aria-label="Tailored interview quick prompts"
                      >
                        <span className="text-xs font-medium text-text-muted">
                          Quick prompts:
                        </span>
                        {activity.interviewPlan.questions.map((q) => {
                          const promptText = `Please ask me this practice question: ${q.question}`;
                          const truncated = questionLabel(q.question);
                          return (
                            <button
                              key={q.id}
                              type="button"
                              disabled={busy}
                              onClick={() => {
                                setMessage(
                                  message.trim()
                                    ? `${message.trim()}\n\n${promptText}`
                                    : promptText,
                                );
                              }}
                              className="max-w-full truncate rounded-full border border-border-default bg-surface px-2.5 py-1 text-xs text-text-secondary hover:border-accent hover:text-accent transition-colors disabled:opacity-40"
                              title={questionLabel(q.question, 300)}
                            >
                              Ask me: {truncated}
                            </button>
                          );
                        })}
                      </div>
                    )}
                </>
              }
            />
          ) : (
            <div
              ref={logRef}
              role="log"
              aria-label="Simulation turns"
              className="max-h-[min(70vh,640px)] space-y-3 overflow-y-auto rounded-xl border border-border-default bg-surface p-3"
            >
              {session.turns.map((turnItem) => (
                <article
                  key={turnItem.id}
                  className={`max-w-[92%] rounded-xl border p-3 ${
                    turnItem.speaker === "learner"
                      ? "ml-auto border-border-default bg-surface-raised"
                      : "border-border-default bg-surface-sunken"
                  }`}
                >
                  <div className="text-xs font-semibold uppercase tracking-[.12em] text-text-muted">
                    {turnItem.speaker === "learner"
                      ? session.learnerRole
                      : turnItem.speaker === "counterpart"
                        ? session.counterpartRole
                        : "Coach"}
                  </div>
                  {turnItem.speaker === "learner" ? (
                    <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-text-secondary">
                      {turnItem.content}
                    </p>
                  ) : (
                    <div className="mt-1 text-sm leading-6 text-text-secondary">
                      <TiptapViewer content={turnItem.content} images={isInterview ? [] : undefined} />
                    </div>
                  )}
                  {showCitations && turnItem.citations && turnItem.citations.length > 0 && (
                    <div className="mt-2 space-y-1">
                      {turnItem.citations.map((citation, index) => (
                        <PracticeCitation
                          key={`${citation.sourceId}:${index}`}
                          programId={programId}
                          citation={citation}
                          canOpen={activity.practiceMode !== "demonstrate" || session.status === "submitted"}
                        />
                      ))}
                    </div>
                  )}
                </article>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
