import { useRef, useState } from "react";

import { useMutation } from "@tanstack/react-query";
import { Play, Send, Users } from "lucide-react";

import { TiptapViewer } from "@/components/TiptapEditor";
import { errorText, unwrap, uuid } from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import { useCitationDisplayStore } from "@/features/reading/stores/citationDisplayStore";
import VaultAPI from "@/lib/api";
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
}: {
  programId: string;
  activity: LearningPracticalActivityDto;
  session: LearningSimulationSessionDto | null;
  onSession: (session: LearningSimulationSessionDto) => void;
}) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const [learnerRole, setLearnerRole] = useState("Incident lead");
  const [counterpartRole, setCounterpartRole] = useState("On-call engineer");
  // Kept per session, so leaving the simulation does not lose the reply.
  const [message, setMessage] = useDraft(session ? `simulation:${session.id}` : null);
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
                  {turnItem.speaker === "learner" ? (
                    <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-text-secondary">
                      {turnItem.content}
                    </p>
                  ) : (
                    <div className="mt-1 text-sm leading-6 text-text-secondary">
                      <TiptapViewer content={turnItem.content} />
                    </div>
                  )}
                  {showCitations && turnItem.citations.map((citation, index) => (
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
                className="min-w-0 flex-1 resize-y rounded-xl border border-border bg-background px-3 py-2.5 text-sm leading-6 outline-hidden focus:border-accent"
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
