import { useEffect, useRef, useState } from "react";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LoaderCircle, Play, Send, Users } from "lucide-react";

import { TiptapViewer } from "@/components/TiptapEditor";
import { useApi } from '@/contexts/ApiContext';
import { learningPracticalWorkspaceKey } from "@/features/learning/api/learningQueryKeys";
import { AnswerTimer } from "@/features/learning/practice/practical/AnswerTimer";
import { errorText, unwrap, uuid } from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import { PracticeCitation } from "@/features/learning/practice/PracticeCitation";
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
}) {
  const api = useApi();
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const client = useQueryClient();
  const isInterview = activity.kind === "interview" || Boolean(activity.interviewPlan);
  const [learnerRole, setLearnerRole] = useState(
    () => activity.interviewPlan?.learnerRole || (activity.kind === "interview" ? "Candidate" : "Incident lead"),
  );
  const [counterpartRole, setCounterpartRole] = useState(
    () => activity.interviewPlan?.counterpartRole || (activity.kind === "interview" ? "Interviewer" : "On-call engineer"),
  );

  useEffect(() => {
    setLearnerRole(activity.interviewPlan?.learnerRole || (activity.kind === "interview" ? "Candidate" : "Incident lead"));
    setCounterpartRole(activity.interviewPlan?.counterpartRole || (activity.kind === "interview" ? "Interviewer" : "On-call engineer"));
  }, [activity.id, activity.kind, activity.interviewPlan?.learnerRole, activity.interviewPlan?.counterpartRole]);

  // Kept per session, so leaving the simulation does not lose the reply.
  const [message, setMessage] = useDraft(session ? `simulation:${session.id}` : null);
  const [error, setError] = useState<string | null>(null);
  const [retryOperation, setRetryOperation] = useState<"start" | "turn" | "finish" | null>(null);
  const busyRef = useRef(false);
  const retryRef = useRef<{
    start?: StartLearningSimulationRequestDto;
    turn?: SendLearningSimulationTurnRequestDto;
    finish?: FinishLearningSimulationRequestDto;
  }>({});
  const refreshAfterError = () => {
    void client.invalidateQueries({ queryKey: learningPracticalWorkspaceKey(programId) });
  };
  useEffect(() => {
    if (session?.status === "submitted") {
      setError(null);
      setRetryOperation(null);
    }
  }, [session?.status]);
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
  const lastQuestionId = session?.turns.filter((item) => item.speaker === "counterpart").at(-1)?.id;
  const begin = async () => {
    if (busyRef.current || !learnerRole.trim() || !counterpartRole.trim()) return;
    busyRef.current = true;
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
      setRetryOperation(null);
      onSession(created);
    } catch (cause) {
      setRetryOperation("start");
      setError(errorText(cause));
    } finally {
      busyRef.current = false;
    }
  };
  const send = async (retryPending = false) => {
    if (
      busyRef.current || session?.status !== "active" ||
      (!message.trim() && !(retryPending && retryRef.current.turn))
    ) return;
    busyRef.current = true;
    setError(null);
    const content = message.trim();
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
    if (busyRef.current || session?.status !== "active" || !session.turns.some((item) => item.speaker === "learner")) return;
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
  return (
    <section className="rounded-2xl border border-border bg-surface p-4 sm:p-5">
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
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
            {isInterview ? "The interviewer opens the conversation and probes your reasoning. Finish the simulation to review feedback against the role’s criteria." : <>The counterpart responds in turns. This is a practice record tied to
            activity revision {activity.revision}; it does not claim operational
            readiness.</>}
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
              maxLength={100}
              disabled={busy}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm"
            />
          </label>
          <label className="text-xs font-medium text-text-secondary">
            Counterpart role
            <input
              value={counterpartRole}
              onChange={(event) => setCounterpartRole(event.target.value)}
              maxLength={100}
              disabled={busy}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm"
            />
          </label>
          <button
            type="button"
            disabled={
              busy || !learnerRole.trim() || !counterpartRole.trim()
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
                disabled={busy || !session.turns.some((item) => item.speaker === "learner")}
                onClick={() => void end()}
                className="rounded-full border border-border px-3 py-2 text-xs"
              >
                Finish simulation
              </button>
            )}
            {session.status !== "active" && onRestart && (
              <button type="button" onClick={onRestart} className="rounded-full border border-border px-3 py-2 text-xs">
                Start another attempt
              </button>
            )}
          </div>
          <div
            aria-label="Simulation turns"
            role="log"
            aria-live="polite"
            aria-busy={busy}
            className="mt-3 max-h-[420px] space-y-3 overflow-y-auto rounded-xl border border-border bg-background/40 p-3"
          >
            {session.turns.length === 0 ? (
              <p className="p-4 text-xs text-text-muted">
                {activity.interviewPlan ? "Introduce yourself to begin the interview." : "Your first response will begin the conversation."}
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
                  {turnItem.speaker === "learner" ? (
                    <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-text-secondary">
                      {turnItem.content}
                    </p>
                  ) : (
                    <div className="mt-1 text-sm leading-6 text-text-secondary">
                      <TiptapViewer content={turnItem.content} images={isInterview ? [] : undefined} />
                    </div>
                  )}
                  {showCitations && turnItem.citations.map((citation, index) => (
                    <PracticeCitation
                      key={`${citation.sourceId}:${index}`}
                      programId={programId}
                      citation={citation}
                      canOpen={activity.practiceMode !== "demonstrate" || session.status === "submitted"}
                    />
                  ))}
                </article>
              ))
            )}
          </div>
          {session.status === "active" && (
            <div className="mt-3 space-y-2">
              {isInterview && lastQuestionId && <AnswerTimer since={lastQuestionId} />}
              {activity.practiceMode !== "demonstrate" && activity.interviewPlan?.questions &&
                activity.interviewPlan.questions.length > 0 && (
                  <div
                    className="flex flex-wrap items-center gap-1.5"
                    role="group"
                    aria-label="Tailored interview quick prompts"
                  >
                    <span className="text-xs font-medium text-text-muted">
                      Quick prompts:
                    </span>
                    {activity.interviewPlan.questions.map((q) => {
                      const promptText = `Please ask me this practice question: ${q.question}`;
                      const truncated =
                        q.question.length > 36
                          ? `${q.question.slice(0, 36)}…`
                          : q.question;
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
                          className="max-w-full truncate rounded-full border border-border bg-background px-2.5 py-1 text-xs text-text-secondary hover:border-accent hover:text-accent transition-colors"
                          title={q.question}
                        >
                          Ask me: {truncated}
                        </button>
                      );
                    })}
                  </div>
                )}
              <div className="flex gap-2">
                <label className="sr-only" htmlFor="simulation-message">
                  Your response
                </label>
                <textarea
                  id="simulation-message"
                  rows={3}
                  maxLength={4000}
                  disabled={busy}
                  value={message}
                  onChange={(event) => setMessage(event.target.value)}
                  onKeyDown={(event) => {
                    if ((event.metaKey || event.ctrlKey) && event.key === "Enter")
                      void send();
                  }}
                  placeholder={`Respond as ${session.learnerRole}…`}
                  className="min-w-0 flex-1 resize-y rounded-xl border border-border bg-background px-3 py-2.5 text-sm leading-6 outline-hidden focus:border-accent"
                />
                <button
                  type="button"
                  aria-label="Send simulation turn"
                  disabled={busy || !message.trim()}
                  onClick={() => void send()}
                  className="self-end rounded-full bg-accent p-3 text-accent-fg disabled:opacity-40"
                >
                  <Send size={15} />
                </button>
              </div>
            </div>
          )}
        </>
      )}
      {busy && (
        <p role="status" className="mt-3 flex items-center gap-2 text-xs text-text-muted">
          <LoaderCircle size={14} className="animate-spin" />
          {start.isPending ? "Starting simulation…" : finish.isPending ? "Preparing your feedback…" : "Your counterpart is responding…"}
        </p>
      )}
      {error && (
        <p
          role="alert"
          className="mt-3 rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700 dark:text-rose-300"
        >
          {error}{" "}
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              retryOperation === "finish"
                ? void end()
                : retryOperation === "turn"
                  ? void send(true)
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
