import { useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { Play, Users } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { useApi } from '@/contexts/ApiContext';
import { learningPracticalWorkspaceKey } from '@/features/learning/api/learningQueryKeys';
import { LEARNER_TEXT_MAX_CHARS } from '@/features/learning/model/learnerTextLimit';
import { AnswerTimer } from '@/features/learning/practice/practical/AnswerTimer';
import { questionLabel } from '@/features/learning/practice/practical/practicalWorkbenchHelpers';
import { PracticeCitation } from '@/features/learning/practice/PracticeCitation';
import { PracticeThread, type ThreadMessage } from '@/features/learning/practice/shared/PracticeThread';
import { StatusLine, type StatusState } from '@/features/learning/practice/shared/StatusLine';
import { useRetry } from '@/features/learning/practice/shared/useRetry';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { newId, unwrap } from '@/lib/api';
import type {
  FinishLearningSimulationRequestDto,
  LearningPracticalActivityDto,
  LearningSimulationSessionDto,
  SendLearningSimulationTurnRequestDto,
  StartLearningSimulationRequestDto,
} from '@/lib/bindings';
import { readDraft, setDraft } from '@/shared/drafts/drafts';

type Operation = 'start' | 'turn' | 'finish';

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
  const isInterview = activity.kind === 'interview' || Boolean(activity.interviewPlan);

  const defaultLearnerRole =
    activity.interviewPlan?.learnerRole || (activity.kind === 'interview' ? 'Candidate' : 'Incident lead');
  const defaultCounterpartRole =
    activity.interviewPlan?.counterpartRole || (activity.kind === 'interview' ? 'Interviewer' : 'On-call engineer');

  const [typedRoles, setTypedRoles] = useState<{ activityId: string; learner?: string; counterpart?: string } | null>(
    null,
  );
  const typed = typedRoles?.activityId === activity.id ? typedRoles : null;
  const learnerRole = typed?.learner ?? defaultLearnerRole;
  const counterpartRole = typed?.counterpart ?? defaultCounterpartRole;

  const draftKey = session ? `simulation:${session.id}` : null;
  const [lastOperation, setLastOperation] = useState<Operation | null>(null);
  const requests = useRef<{
    start?: StartLearningSimulationRequestDto;
    turn?: SendLearningSimulationTurnRequestDto;
    finish?: FinishLearningSimulationRequestDto;
  }>({});

  const refreshAfterError = () => {
    void client.invalidateQueries({ queryKey: learningPracticalWorkspaceKey(programId) });
  };

  const start = useRetry(async (request: StartLearningSimulationRequestDto) => {
    try {
      const created = unwrap(await api.startLearningSimulation(request));
      requests.current.start = undefined;
      onSession(created);
      return true;
    } catch (cause) {
      refreshAfterError();
      throw cause;
    }
  });

  const turn = useRetry(async (request: SendLearningSimulationTurnRequestDto) => {
    try {
      const updated = unwrap(await api.sendLearningSimulationTurn(request));
      requests.current.turn = undefined;
      onSession(updated);
      const key = `simulation:${request.sessionId}`;
      if (readDraft(key).trim() === request.content) setDraft(key, '');
      return true;
    } catch (cause) {
      refreshAfterError();
      throw cause;
    }
  });

  const finish = useRetry(async (request: FinishLearningSimulationRequestDto) => {
    try {
      const finished = unwrap(await api.finishLearningSimulation(request));
      requests.current.finish = undefined;
      onSession(finished);
      return true;
    } catch (cause) {
      refreshAfterError();
      throw cause;
    }
  });

  const busy = start.busy || turn.busy || finish.busy;

  const busyLabel = start.busy
    ? isInterview
      ? 'Starting the interview…'
      : 'Starting simulation…'
    : finish.busy
      ? 'Preparing your feedback…'
      : turn.busy
        ? isInterview
          ? 'The interviewer is responding…'
          : 'Your counterpart is responding…'
        : null;

  const failed = lastOperation === null ? null : { start, turn, finish }[lastOperation];
  const status: StatusState = busyLabel
    ? { kind: 'busy', label: busyLabel }
    : failed?.error
      ? failed.conflict
        ? {
            kind: 'error',
            message: failed.error,
            reload: { label: 'Reload simulation', run: () => { refreshAfterError(); setLastOperation(null); } },
          }
        : { kind: 'error', message: failed.error, retry: failed.retry }
      : { kind: 'idle' };

  const lastQuestionId = session?.turns.filter((item) => item.speaker === 'counterpart').at(-1)?.id;
  const hasLearnerTurn = Boolean(session?.turns.some((item) => item.speaker === 'learner'));

  const begin = () => {
    if (!learnerRole.trim() || !counterpartRole.trim()) return;
    const fingerprint = `${activity.id}:${learnerRole.trim()}:${counterpartRole.trim()}`;
    let request = requests.current.start;
    if (!request || `${request.activityId}:${request.learnerRole}:${request.counterpartRole}` !== fingerprint) {
      request = {
        operationId: newId(),
        sessionId: newId(),
        programId,
        activityId: activity.id,
        expectedActivityRevision: activity.revision,
        practiceSessionId: null,
        learnerRole: learnerRole.trim(),
        counterpartRole: counterpartRole.trim(),
      };
      requests.current.start = request;
    }
    setLastOperation('start');
    void start.run(request);
  };

  const send = async (text: string) => {
    if (session?.status !== 'active') throw new Error('This attempt is not active.');
    const content = text.trim();
    let request = requests.current.turn;
    if (
      request?.sessionId !== session.id ||
      request.expectedRevision !== session.revision ||
      request.content !== content
    ) {
      request = {
        operationId: newId(),
        programId,
        sessionId: session.id,
        expectedRevision: session.revision,
        content,
      };
      requests.current.turn = request;
    }
    setLastOperation('turn');
    if (!(await turn.run(request))) throw new Error('The reply failed.');
  };

  const end = () => {
    if (session?.status !== 'active' || !hasLearnerTurn) return;
    let request = requests.current.finish;
    if (request?.sessionId !== session.id || request.expectedRevision !== session.revision) {
      request = {
        operationId: newId(),
        programId,
        sessionId: session.id,
        expectedRevision: session.revision,
      };
      requests.current.finish = request;
    }
    setLastOperation('finish');
    void finish.run(request);
  };

  const canOpenCitations = (current: LearningSimulationSessionDto) =>
    activity.practiceMode !== 'demonstrate' || current.status === 'submitted';

  const turnBody = (current: LearningSimulationSessionDto, id: string) => {
    const item = current.turns.find((candidate) => candidate.id === id);
    if (!item) return null;
    return (
      <>
        {item.speaker === 'learner' ? (
          <p className="whitespace-pre-wrap text-sm leading-6 text-text-secondary">{item.content}</p>
        ) : (
          <div className="text-sm leading-6 text-text-secondary">
            <TiptapViewer content={item.content} images={isInterview ? [] : undefined} />
          </div>
        )}
        {showCitations && item.citations && item.citations.length > 0 && (
          <div className="mt-2 space-y-1">
            {item.citations.map((citation, index) => (
              <PracticeCitation
                key={`${citation.sourceId}:${index}`}
                programId={programId}
                citation={citation}
                canOpen={canOpenCitations(current)}
              />
            ))}
          </div>
        )}
      </>
    );
  };

  const roleName = (current: LearningSimulationSessionDto, speaker: string) =>
    speaker === 'learner' ? current.learnerRole : speaker === 'counterpart' ? current.counterpartRole : 'Coach';

  const messages: ThreadMessage[] =
    session?.turns.map((item) => ({
      id: item.id,
      speaker: item.speaker === 'learner' ? 'learner' : item.speaker === 'counterpart' ? 'counterpart' : 'coach',
      content: item.content,
      name: roleName(session, item.speaker),
    })) ?? [];

  const questions = activity.interviewPlan?.questions ?? [];
  const finishReason =
    session && !hasLearnerTurn ? `Send at least one answer to ${isInterview ? 'finish the interview' : 'finish'}.` : null;
  const finishLabel = isInterview ? 'Finish interview' : 'Finish simulation';

  return (
    <section className="rounded-2xl border border-border-default bg-surface p-4 sm:p-5">
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent-muted text-accent">
          <Users size={18} aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">
            {isInterview ? 'Company interview practice' : 'Role-based simulation'}
          </div>
          <h3 className="mt-1 font-serif text-xl text-text-primary">
            {isInterview ? 'Answer one question at a time' : 'Take a role, respond to the situation'}
          </h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            {isInterview ? (
              'The interviewer opens the conversation and probes your reasoning. Finish the simulation to review feedback against the role’s criteria.'
            ) : (
              <>
                The counterpart responds in turns. This is a practice record tied to activity revision {activity.revision};
                it does not claim operational readiness.
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
              onChange={(event) => setTypedRoles({ ...typed, activityId: activity.id, learner: event.target.value })}
              maxLength={100}
              disabled={busy}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2.5 text-sm text-text-primary"
            />
          </label>
          <label className="text-xs font-medium text-text-secondary">
            Counterpart role
            <input
              value={counterpartRole}
              onChange={(event) =>
                setTypedRoles({ ...typed, activityId: activity.id, counterpart: event.target.value })
              }
              maxLength={100}
              disabled={busy}
              className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2.5 text-sm text-text-primary"
            />
          </label>
          <div className="space-y-1.5 sm:col-span-2">
            <button
              type="button"
              disabled={busy || !learnerRole.trim() || !counterpartRole.trim()}
              onClick={begin}
              className="inline-flex w-fit items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45"
            >
              <Play size={13} aria-hidden="true" /> {isInterview ? 'Start interview' : 'Start simulation'}
            </button>
            {!busy && (!learnerRole.trim() || !counterpartRole.trim()) && (
              <p className="text-xs text-text-secondary">Name both roles to start.</p>
            )}
          </div>
          <div className="sm:col-span-2">
            <StatusLine state={status} />
          </div>
        </div>
      ) : (
        <div className="mt-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl bg-surface-sunken px-4 py-3">
            <div className="text-xs text-text-secondary">
              <strong className="text-text-primary">{session.learnerRole}</strong> with{' '}
              <strong className="text-text-primary">{session.counterpartRole}</strong> ·{' '}
              {session.status === 'active' ? 'in progress' : 'finished'}
            </div>
            {session.status === 'active' && (
              <div className="space-y-1">
                <button
                  type="button"
                  disabled={busy || !hasLearnerTurn}
                  onClick={end}
                  className="rounded-full border border-border-default bg-surface px-3 py-2 text-xs font-medium text-text-primary hover:border-accent hover:text-accent disabled:opacity-40"
                >
                  {finishLabel}
                </button>
                {finishReason && <p className="text-xs text-text-secondary">{finishReason}</p>}
              </div>
            )}
            {session.status !== 'active' && onRestart && (
              <button
                type="button"
                onClick={onRestart}
                className="rounded-full border border-border-default bg-surface px-3 py-2 text-xs font-medium text-text-primary hover:border-accent hover:text-accent"
              >
                Start another attempt
              </button>
            )}
          </div>

          {session.status === 'active' ? (
            <PracticeThread
              label="Simulation turns"
              messages={messages}
              onSend={send}
              busy={busyLabel}
              error={status.kind === 'error' ? { message: status.message, retry: status.retry, reload: status.reload } : null}
              placeholder={`Respond as ${session.learnerRole}…`}
              draftKey={draftKey ?? undefined}
              maxLength={LEARNER_TEXT_MAX_CHARS}
              suggestion={
                messages.length === 0
                  ? activity.interviewPlan
                    ? 'Introduce yourself to begin the interview.'
                    : 'Your first response will begin the conversation.'
                  : null
              }
              renderMessage={(message) => turnBody(session, message.id)}
              composerTop={
                <>
                  {isInterview && lastQuestionId && !finish.busy && (
                    <AnswerTimer since={lastQuestionId} running={!turn.busy} />
                  )}
                  {activity.practiceMode !== 'demonstrate' && questions.length > 0 && (
                    <div
                      className="flex flex-wrap items-center gap-1.5 pt-1"
                      role="group"
                      aria-label="Tailored interview quick prompts"
                    >
                      <span className="text-xs font-medium text-text-muted">Quick prompts:</span>
                      {questions.map((q) => {
                        const promptText = `Please ask me this practice question: ${q.question}`;
                        return (
                          <button
                            key={q.id}
                            type="button"
                            disabled={busy}
                            onClick={() => {
                              const current = readDraft(`simulation:${session.id}`).trim();
                              setDraft(`simulation:${session.id}`, current ? `${current}\n\n${promptText}` : promptText);
                            }}
                            className="max-w-full truncate rounded-full border border-border-default bg-surface px-2.5 py-1 text-xs text-text-secondary transition-colors hover:border-accent hover:text-accent disabled:opacity-40"
                            title={questionLabel(q.question, 300)}
                          >
                            Ask me: {questionLabel(q.question)}
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
              role="log"
              aria-label="Simulation turns"
              className="max-h-[min(70vh,640px)] space-y-3 overflow-y-auto rounded-xl border border-border-default bg-surface p-3"
            >
              {session.turns.map((item) => (
                <article
                  key={item.id}
                  className={`max-w-[92%] rounded-xl border p-3 ${
                    item.speaker === 'learner'
                      ? 'ml-auto border-border-default bg-surface-raised'
                      : 'border-border-default bg-surface-sunken'
                  }`}
                >
                  <div className="text-xs font-semibold uppercase tracking-[.12em] text-text-muted">
                    {roleName(session, item.speaker)}
                  </div>
                  <div className="mt-1">{turnBody(session, item.id)}</div>
                </article>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
