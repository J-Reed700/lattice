import { newId } from '@/lib/api';
import { useEffect, useRef, useState } from 'react';

import {
  AlertCircle,
  ArrowDownRight,
  Check,
  CircleHelp,
  Lightbulb,
  MessageCircle,
  ShieldCheck,
  Sparkles,
  Unlock,
} from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { EmbeddedPracticeView } from '@/features/learning/practice/EmbeddedPracticeView';
import {
  ConfirmDialog,
  PreviousFeedback,
  ResultPanel,
  RubricPanel,
} from '@/features/learning/practice/PracticeRubricPanel';
import {
  CitationCards,
  ExactSourceReader,
  FrozenSourceRail,
  sourceForVersion,
} from '@/features/learning/practice/PracticeSourceRail';
import { PracticeThread, type PracticeThreadTurn } from '@/features/learning/practice/shared/PracticeThread';
import { SaveAsReviewCard } from '@/features/learning/practice/shared/SaveAsReviewCard';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import { usePracticeSession } from '@/features/learning/practice/usePracticeSession';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type {
  LearningLessonDto,
  LearningPracticeHintLevel,
  LearningPracticeMode,
  LearningPracticeTaskKind,
  LearningProgramDto,
  SaveLearningPracticeArtifactRequestDto,
} from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';
import { useDraft } from '@/shared/drafts/drafts';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

type Mode = LearningPracticeMode;

const MODES: { id: Mode; title: string; aid: string; description: string }[] = [
  { id: 'explore', title: 'Explore', aid: 'Help: Full help', description: 'Ask questions, use any hint, and reveal the worked solution after a deliberate confirmation.' },
  { id: 'practice', title: 'Practice', aid: 'Help: Hints only', description: 'Ask questions and use scaffolded hints. Worked explanations and the solution stay closed.' },
  { id: 'demonstrate', title: 'Demonstrate', aid: 'Help: No help', description: 'Write and submit independently. Source opening, tutor hints, and solutions are closed during the attempt.' },
];

const HINTS: { id: LearningPracticeHintLevel; title: string; prompt: string }[] = [
  { id: 'orienting_question', title: 'Orienting question', prompt: 'Give me an orienting question that helps me notice where to begin, without giving away the answer.' },
  { id: 'concept_or_source', title: 'Concept or source', prompt: 'Point me toward a relevant concept or exact saved source, without solving the task.' },
  { id: 'partial_strategy', title: 'Partial strategy', prompt: 'Offer one partial strategy step, leaving the key reasoning for me.' },
  { id: 'worked_explanation', title: 'Worked explanation', prompt: 'Give a worked explanation. Clearly label it as substantial assistance.' },
];


function dateLabel(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'Time unavailable' : date.toLocaleString();
}

function assistanceLabel(kind: string) {
  return (
    ({
      source_opened: 'Opened a frozen source',
      hint: 'Requested a tutor hint',
      solution_revealed: 'Revealed worked solution',
      mode_changed: 'Changed aid contract',
    } as Record<string, string>)[kind] ?? kind
  );
}

export function PracticeWorkbenchPanel({
  program,
  lesson,
  taskKind = 'independent',
  embedded = false,
  onContinue,
}: {
  program: LearningProgramDto;
  lesson: LearningLessonDto | undefined;
  taskKind?: LearningPracticeTaskKind;
  embedded?: boolean;
  onContinue?: () => void;
}) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const programId = program.summary.id;
  const lessonId = lesson?.id ?? '';
  const sourceWorkspace = useLearningSourceWorkspace(programId);

  const sessionState = usePracticeSession({ program, lesson, taskKind, embedded });
  const {
    sessionId,
    setSessionId,
    sessionQuery,
    session,
    summary,
    active,
    mode,
    modeChoice,
    setModeChoice,
    canTutor,
    solutionAllowed,
    sourceOpeningAllowed,
    busyState,
    busyLabel,
    actionError,
    setActionError,
    conflict,
    setConflict,
    retryAction,
    setRetryAction,
    solutionConfirm,
    setSolutionConfirm,
    selectedSource,
    setSelectedSource,
    hintUsed,
    revisionRef,
    perform,
    startSession,
    switchMode,
    sendTutor,
    doOpenSource,
    doReveal,
    doSubmit,
    decide,
    workspace,
    saveMutation,
  } = sessionState;

  const [text, setText] = useState('');
  const [savedText, setSavedText] = useState('');
  const [composer, setComposer] = useDraft(sessionId ? `practice-tutor:${sessionId}` : null);
  const conflictRef = useRef(false);
  const solutionTriggerRef = useRef<HTMLButtonElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveFlightRef = useRef<Promise<boolean> | null>(null);
  const saveRequestRef = useRef<{ fingerprint: string; request: SaveLearningPracticeArtifactRequestDto } | null>(null);
  const textRef = useRef(text);
  const savedRef = useRef(savedText);
  const hydratedSessionRef = useRef<string | null>(null);
  const hydratedRevisionRef = useRef<number>(-1);

  const sourceRequest = selectedSource ? { programId, sourceId: selectedSource.sourceId, versionId: selectedSource.versionId } : null;
  const sourceVersion = useLearningSourceVersion(sourceRequest);

  useEffect(() => { textRef.current = text; }, [text]);
  useEffect(() => { savedRef.current = savedText; }, [savedText]);

  useEffect(() => {
    if (!session) return;
    if (session.summary.id !== sessionId) return;
    if (hydratedSessionRef.current !== session.summary.id) {
      hydratedSessionRef.current = session.summary.id;
      hydratedRevisionRef.current = session.artifact.revision;
      setText(session.artifact.text);
      setSavedText(session.artifact.text);
      textRef.current = session.artifact.text;
      savedRef.current = session.artifact.text;
      saveRequestRef.current = null;
    } else if (session.artifact.revision > hydratedRevisionRef.current && textRef.current === savedRef.current) {
      hydratedRevisionRef.current = session.artifact.revision;
      setText(session.artifact.text);
      textRef.current = session.artifact.text;
      setSavedText(session.artifact.text);
      savedRef.current = session.artifact.text;
    }
  }, [session, sessionId]);

  const saveNow = async (): Promise<boolean> => {
    if (!summary || !active || textRef.current === savedRef.current) return true;
    if (saveFlightRef.current) return saveFlightRef.current;
    const payload = textRef.current;
    const expectedRevision = revisionRef.current || summary.revision;
    const fingerprint = `${summary.id}:${expectedRevision}:${payload}`;
    if (saveRequestRef.current?.fingerprint !== fingerprint) {
      saveRequestRef.current = {
        fingerprint,
        request: { operationId: newId(), programId, sessionId: summary.id, expectedRevision, text: payload },
      };
    }
    const request = saveRequestRef.current.request;
    const flight = (async () => {
      const ok = await perform('saving', async () => {
        const updated = await saveMutation.mutateAsync(request);
        const updatedSummary = updated.sessions.find((item) => item.id === request.sessionId);
        if (updatedSummary) {
          revisionRef.current = updatedSummary.revision;
          hydratedRevisionRef.current = updatedSummary.artifactRevision;
        }
        await sessionQuery.refetch();
        setSavedText(payload);
        savedRef.current = payload;
        saveRequestRef.current = null;
      });
      if (!ok) {
        if (!conflictRef.current) {
          const retry = {
            label: 'Retry save',
            run: async () => {
              await saveNow();
            },
          };
          setRetryAction(() => retry);
        }
        return false;
      }
      return true;
    })();
    saveFlightRef.current = flight;
    try {
      const ok = await flight;
      saveFlightRef.current = null;
      if (ok && textRef.current !== payload) return await saveNow();
      return ok;
    } finally {
      if (saveFlightRef.current === flight) saveFlightRef.current = null;
    }
  };

  const saveNowRef = useRef(saveNow);
  saveNowRef.current = saveNow;
  useEffect(() => registerPendingSave(() => saveNowRef.current()), [sessionId]);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (!active || text === savedText) return;
    timerRef.current = setTimeout(() => {
      void saveNowRef.current();
    }, 650);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [text, savedText, active, sessionId]);

  const sessionList = workspace.data?.sessions ?? [];
  if (workspace.isLoading) {
    return <div role="status" className="rounded-2xl border border-border-default bg-surface p-8 text-xs text-text-muted">Opening your practice workbench…</div>;
  }
  if (workspace.isError) {
    return (
      <div className="rounded-2xl border border-border-default bg-surface p-6">
        <StatusLine error={workspace.error.message} onRetry={() => void workspace.refetch()} retryLabel="Retry workbench" retryAriaLabel="Retry workbench" />
      </div>
    );
  }

  if (embedded) {
    return (
      <EmbeddedPracticeView
        program={program}
        lesson={lesson}
        sessionState={sessionState}
        text={text}
        savedText={savedText}
        setText={setText}
        textRef={textRef}
        saveNow={saveNow}
        onContinue={onContinue}
        hints={HINTS}
      />
    );
  }

  const threadTurns: PracticeThreadTurn[] = session
    ? session.tutorTurns.map((turn, index) => ({
        id: turn.id,
        speaker: 'coach',
        speakerLabel: turn.hintLevel
          ? (HINTS.find((hint) => hint.id === turn.hintLevel)?.title ?? `Hint ${index + 1}`)
          : turn.requestKind === 'critique'
            ? 'Critique'
            : 'Tutor',
        content: turn.response,
        badge: turn.modelName ? `${turn.modelName} · ${dateLabel(turn.createdAt)}` : null,
        extra:
          showCitations && turn.citations.length > 0 ? (
            <CitationCards
              disabled={!sourceOpeningAllowed || Boolean(busyLabel)}
              citations={turn.citations}
              onOpen={(citation) => {
                const src = sourceForVersion(sourceWorkspace.data?.sources ?? [], citation.sourceId, citation.versionId);
                if (src) void doOpenSource(src, citation.versionId, saveNow);
              }}
            />
          ) : null,
      }))
    : [];

  return (
    <section className="min-w-0 space-y-5" aria-label="Grounded Practice Workbench">
      <header className="rounded-2xl border border-border-default bg-surface p-5 sm:p-7">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.16em] text-accent">
              <Sparkles size={14} /> Grounded Practice Workbench
            </div>
            <h3 className="mt-2 font-serif text-2xl leading-tight text-text-primary">
              Make your thinking visible
            </h3>
            <p className="mt-2 max-w-2xl text-xs leading-6 text-text-secondary">
              Work on the lesson assignment, get specific feedback, and revise your response. Tutor assistance is recorded with the attempt; feedback is provisional evidence, not a mastery score.
            </p>
          </div>
          {sessionList.length > 0 && (
            <label className="min-w-0 max-w-full text-xs text-text-muted">
              Attempt history
              <select
                aria-label="Select an attempt"
                value={sessionId ?? ''}
                onChange={(event) => {
                  void saveNow().then((ok) => {
                    if (ok) setSessionId(event.target.value || null);
                  });
                }}
                className="mt-1 block w-full max-w-full rounded-lg border border-border-default bg-surface px-3 py-2 text-xs text-text-primary"
              >
                <option value="">Choose an attempt</option>
                {sessionList
                  .filter((item) => item.lessonId === lessonId && (item.taskKind ?? 'independent') === taskKind)
                  .map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.status === 'submitted' ? 'Submitted' : 'In progress'} · {item.mode} · {dateLabel(item.createdAt)}
                    </option>
                  ))}
              </select>
            </label>
          )}
        </div>

        {!lesson ? (
          <p className="mt-5 rounded-xl bg-surface-sunken p-4 text-xs text-text-muted">
            Choose a lesson to begin a grounded attempt.
          </p>
        ) : lesson.preparation !== 'ready' ? (
          <div className="mt-5 rounded-xl border border-warning-muted bg-surface-sunken p-4">
            <p className="text-xs font-medium text-text-primary">Prepare this lesson first</p>
            <p className="mt-1 text-xs leading-5 text-text-secondary">
              The workbench uses the lesson’s prepared prompt and frozen source versions. Preparing a lesson does not create an attempt.
            </p>
          </div>
        ) : !sessionId ? (
          <div className="mt-5 grid gap-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
            <fieldset className="min-w-0">
              <legend className="text-xs font-semibold text-text-primary">Choose your aid contract</legend>
              <div className="mt-2 grid gap-2 sm:grid-cols-3">
                {MODES.map((item) => (
                  <label
                    key={item.id}
                    className={`cursor-pointer rounded-xl border p-3 transition ${
                      modeChoice === item.id ? 'border-accent bg-accent-muted' : 'border-border-default bg-surface'
                    }`}
                  >
                    <span className="flex items-center gap-2">
                      <input
                        type="radio"
                        name="new-session-mode"
                        value={item.id}
                        checked={modeChoice === item.id}
                        onChange={() => setModeChoice(item.id)}
                        className="accent-accent"
                      />
                      <span className="text-xs font-semibold text-text-primary">{item.title}</span>
                    </span>
                    <span className="mt-1 block text-xs font-semibold uppercase tracking-wider text-accent">
                      {item.aid}
                    </span>
                    <span className="mt-1 block text-xs leading-5 text-text-secondary">{item.description}</span>
                  </label>
                ))}
              </div>
            </fieldset>
            <button
              type="button"
              disabled={Boolean(busyLabel)}
              onClick={() => void startSession()}
              className="rounded-full bg-accent px-5 py-3 text-xs font-semibold text-accent-fg disabled:opacity-50"
            >
              {busyState === 'starting' ? 'Starting…' : 'Start an attempt'}
            </button>
          </div>
        ) : sessionQuery.isLoading ? (
          <p role="status" className="mt-5 text-xs text-text-muted">Loading the selected attempt…</p>
        ) : session ? (
          <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-border-default pt-4">
            <div>
              <span className="text-xs font-semibold text-text-primary">{lesson.title}</span>
              <span className="ml-2 rounded-full bg-surface-sunken px-2 py-1 text-xs uppercase tracking-wider text-text-muted">
                {summary?.status === 'submitted' ? 'Submitted' : 'In progress'}
              </span>
            </div>
            {summary?.status === 'submitted' && (
              <button
                type="button"
                disabled={Boolean(busyLabel)}
                onClick={() => void startSession()}
                className="rounded-full border border-accent px-4 py-2 text-xs font-semibold text-accent hover:bg-accent-muted"
              >
                Revise in a new attempt
              </button>
            )}
          </div>
        ) : null}
      </header>

      {lesson?.preparation === 'ready' &&
        sessionId &&
        (sessionQuery.isLoading ? (
          <div role="status" className="rounded-2xl border border-border-default bg-surface p-7 text-xs text-text-muted">
            Loading saved attempt…
          </div>
        ) : sessionQuery.isError ? (
          <div className="rounded-2xl border border-border-default bg-surface p-5">
            <StatusLine error={`Could not load attempt: ${sessionQuery.error.message}`} onRetry={() => void sessionQuery.refetch()} retryLabel="Retry" retryAriaLabel="Retry" />
          </div>
        ) : session ? (
          <div className="grid min-w-0 gap-5 xl:grid-cols-[minmax(0,1fr)_330px]">
            <div className="min-w-0 space-y-5">
              <section className="rounded-2xl border border-border-default bg-surface p-5 sm:p-7">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Task prompt</div>
                    <h4 className="mt-1 font-serif text-xl text-text-primary">Respond to the problem</h4>
                  </div>
                  <span className="rounded-full bg-surface-sunken px-3 py-1.5 text-xs font-medium uppercase tracking-wider text-text-muted">
                    {summary?.mode} mode · aid is recorded
                  </span>
                </div>
                <div className="mt-4 rounded-xl bg-surface-sunken p-4 text-sm leading-7 text-text-secondary">
                  <TiptapViewer content={session.taskPrompt} />
                </div>
                <div className="mt-3 rounded-lg border-l-2 border-accent pl-3">
                  <span className="text-xs font-semibold uppercase tracking-wider text-text-muted">Lesson objective</span>
                  <p className="mt-1 text-xs leading-5 text-text-secondary">{session.lessonObjective}</p>
                </div>
                <div className="mt-5 flex flex-wrap gap-2" role="group" aria-label="Session mode">
                  {MODES.filter((item) => !session?.summary.revisesSessionId || item.id !== 'demonstrate').map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      aria-pressed={mode === item.id}
                      disabled={!active || Boolean(busyLabel)}
                      onClick={() => void switchMode(item.id, saveNow)}
                      className={`rounded-full border px-3 py-2 text-xs font-semibold transition disabled:cursor-default disabled:opacity-70 ${
                        mode === item.id
                          ? 'border-accent bg-accent text-accent-fg'
                          : 'border-border-default text-text-secondary hover:border-accent'
                      }`}
                    >
                      {item.title}
                    </button>
                  ))}
                </div>
                <p className="mt-2 text-xs leading-5 text-text-muted">
                  Aid contract: {MODES.find((item) => item.id === mode)?.description}
                </p>
              </section>

              <section className="overflow-hidden rounded-2xl border border-border-default bg-surface">
                <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-default px-5 py-4">
                  <div>
                    <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Your artifact</div>
                    <h4 className="mt-1 font-serif text-xl text-text-primary">Work through the reasoning</h4>
                  </div>
                  <div className="flex items-center gap-2 text-xs text-text-muted" role="status" aria-live="polite">
                    {busyState === 'saving' || saveMutation.isPending ? (
                      <>
                        <span className="h-2 w-2 animate-pulse rounded-full bg-accent" /> Saving…
                      </>
                    ) : actionError && retryAction?.label === 'Retry save' ? (
                      <>
                        <AlertCircle size={13} className="text-danger-fg" /> Save needs attention
                      </>
                    ) : text === savedText ? (
                      <>
                        <Check size={13} className="text-success-fg" /> Saved
                      </>
                    ) : (
                      'Unsaved changes'
                    )}
                  </div>
                </div>
                <label htmlFor="workbench-artifact" className="sr-only">
                  Your response
                </label>
                <textarea
                  id="workbench-artifact"
                  aria-label="Your response"
                  value={text}
                  disabled={!active}
                  onChange={(event) => {
                    setText(event.target.value);
                    textRef.current = event.target.value;
                    setActionError('');
                    setRetryAction(null);
                  }}
                  placeholder="Write your response, reasoning, or working here…"
                  className="min-h-[250px] w-full resize-y bg-transparent px-5 py-5 text-sm leading-7 text-text-primary placeholder:text-text-muted focus:outline-hidden focus:ring-2 focus:ring-accent disabled:opacity-70 sm:min-h-[330px] sm:px-7"
                />
                <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border-default bg-surface-sunken px-5 py-3">
                  <p className="text-xs leading-5 text-text-muted">
                    Your writing autosaves to this attempt. Submitting freezes this version for feedback.
                  </p>
                  <div className="flex flex-wrap gap-2">
                    {conflict && (
                      <button
                        type="button"
                        onClick={handleAsyncEvent(async () => {
                          await sessionQuery.refetch();
                          setConflict(false);
                          conflictRef.current = false;
                        })}
                        className="rounded-full border border-warning-muted px-3 py-2 text-xs font-semibold text-warning-fg"
                      >
                        Reload current revision · keep my text
                      </button>
                    )}
                    {retryAction && (
                      <button
                        type="button"
                        onClick={() => void retryAction.run()}
                        className="rounded-full border border-danger-muted px-3 py-2 text-xs font-semibold text-danger-fg"
                      >
                        {retryAction.label}
                      </button>
                    )}
                    {active && (
                      <button
                        type="button"
                        disabled={Boolean(busyLabel) || !text.trim()}
                        onClick={() => void doSubmit(text, saveNow)}
                        className="rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-45"
                      >
                        {busyState === 'submitting' ? 'Submitting…' : 'Submit response'}
                      </button>
                    )}
                  </div>
                </div>
                {actionError && <p role="alert" className="px-5 pb-4 text-xs text-danger-fg">{actionError}</p>}
              </section>

              {session.result && <ResultPanel result={session.result} />}

              {session.summary.revisesSessionId && <PreviousFeedback session={session} />}

              <section className="rounded-2xl border border-border-default bg-surface p-5 sm:p-6">
                <div className="flex items-center gap-2 text-accent">
                  <MessageCircle size={15} />
                  <h4 className="font-serif text-xl text-text-primary">Tutor conversation</h4>
                </div>
                <div className="mt-4">
                  <PracticeThread
                    conversationId={session.summary.id}
                    turns={threadTurns}
                    draftKey={sessionId ? `practice-tutor:${sessionId}` : null}
                    placeholder={canTutor ? 'Ask about a step, assumption, or source…' : 'Tutor assistance is closed in Demonstrate mode.'}
                    sendLabel="Ask tutor"
                    busyLabel={busyState === 'tutor' ? 'The tutor is responding…' : null}
                    disabled={!canTutor || Boolean(busyLabel)}
                    disabledReason={!canTutor ? 'Tutor assistance is closed in Demonstrate mode.' : null}
                    inputLabel="Ask a question or request critique"
                    clearOnSend={false}
                    onSend={async (prompt) => {
                      if (await sendTutor('question', prompt, undefined, saveNow)) {
                        setComposer('');
                      }
                    }}
                    renderTurnContent={(turn) => (
                      <div className="mt-1 text-sm leading-6 text-text-secondary">
                        <TiptapViewer content={turn.content} />
                      </div>
                    )}
                    composerTop={
                      active && (
                        <div className="mb-3 space-y-3">
                          <div className="flex flex-wrap justify-end gap-2">
                            <button
                              type="button"
                              disabled={!canTutor || !composer.trim() || Boolean(busyLabel)}
                              onClick={handleAsyncEvent(async () => {
                                if (await sendTutor('critique', composer, undefined, saveNow)) {
                                  setComposer('');
                                }
                              })}
                              className="rounded-full border border-border-default px-3 py-1.5 text-xs font-semibold text-text-secondary hover:border-accent hover:text-accent disabled:opacity-40"
                            >
                              Request critique
                            </button>
                          </div>
                          <div className="rounded-xl border border-border-default bg-surface-sunken p-3">
                            <div className="flex items-center gap-2">
                              <Lightbulb size={14} className="text-accent" />
                              <span className="text-xs font-semibold text-text-primary">Hint ladder</span>
                              <span className="text-xs text-text-muted">Each rung is a recorded aid</span>
                            </div>
                            <div className="mt-2.5 grid gap-2 sm:grid-cols-2">
                              {HINTS.map((hint, index) => {
                                const locked = mode === 'practice' && hint.id === 'worked_explanation';
                                return (
                                  <button
                                    key={hint.id}
                                    type="button"
                                    disabled={!canTutor || Boolean(busyLabel) || hintUsed.includes(hint.id) || locked}
                                    onClick={() => void sendTutor('hint', hint.prompt, hint.id, saveNow)}
                                    className="flex min-w-0 items-center gap-2 rounded-lg border border-border-default bg-surface px-3 py-2 text-left text-xs text-text-secondary hover:border-accent disabled:opacity-45"
                                  >
                                    <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent-muted text-xs font-semibold text-accent">
                                      {index + 1}
                                    </span>
                                    <span className="min-w-0 flex-1">
                                      {hint.title}
                                      {locked && <span className="block text-xs text-text-muted">Available in Explore</span>}
                                    </span>
                                    {hintUsed.includes(hint.id) && <Check size={13} className="shrink-0 text-success-fg" />}
                                  </button>
                                );
                              })}
                            </div>
                          </div>
                        </div>
                      )
                    }
                  />
                </div>
              </section>

              {session.revealedSolution && (
                <section className="rounded-2xl border border-border-default bg-surface p-5">
                  <div className="flex items-center gap-2 text-accent">
                    <Unlock size={15} />
                    <h4 className="font-serif text-xl text-text-primary">Worked solution revealed</h4>
                  </div>
                  <div className="mt-3 text-sm leading-7 text-text-secondary">
                    <TiptapViewer content={session.revealedSolution} />
                  </div>
                  {showCitations && session.revealedSolutionCitations.length > 0 && (
                    <CitationCards
                      disabled={!sourceOpeningAllowed || Boolean(busyLabel)}
                      citations={session.revealedSolutionCitations}
                      onOpen={(citation) => {
                        const src = sourceForVersion(sourceWorkspace.data?.sources ?? [], citation.sourceId, citation.versionId);
                        if (src) void doOpenSource(src, citation.versionId, saveNow);
                      }}
                    />
                  )}
                </section>
              )}

              {session.proposals.length > 0 && (
                <section className="rounded-2xl border border-border-default bg-surface p-5">
                  <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Follow-up ideas</div>
                  <h4 className="mt-1 font-serif text-xl text-text-primary">Tutor proposals</h4>
                  <div className="mt-4 space-y-3">
                    {session.proposals.map((proposal) => (
                      <article key={proposal.id} className="rounded-xl border border-border-default bg-surface-sunken p-4">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="rounded-full bg-accent-muted px-2.5 py-1 text-xs font-semibold uppercase tracking-wider text-accent">
                            {proposal.kind === 'follow_up' ? 'Follow-up' : 'Possible misconception'}
                          </span>
                          <span className="text-xs text-text-muted">{proposal.status}</span>
                        </div>
                        <p className="mt-2 text-sm leading-6 text-text-secondary">{proposal.text}</p>
                        {proposal.evidenceQuote && (
                          <blockquote className="mt-3 border-l-2 border-accent pl-3 text-xs italic leading-5 text-text-muted">
                            “{proposal.evidenceQuote}”
                          </blockquote>
                        )}
                        {active && proposal.status === 'pending' && (
                          <div className="mt-3 flex gap-2">
                            <button
                              type="button"
                              disabled={Boolean(busyLabel)}
                              onClick={() => void decide(proposal.id, true, saveNow)}
                              className="rounded-full bg-accent px-3 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50"
                            >
                              Accept idea
                            </button>
                            <button
                              type="button"
                              disabled={Boolean(busyLabel)}
                              onClick={() => void decide(proposal.id, false, saveNow)}
                              className="rounded-full border border-border-default px-3 py-2 text-xs font-semibold text-text-secondary disabled:opacity-50"
                            >
                              Not useful
                            </button>
                          </div>
                        )}
                        {proposal.kind === 'misconception' && proposal.status === 'accepted' && lessonId && (
                          <SaveAsReviewCard
                            programId={programId}
                            lessonId={lessonId}
                            promptText={`What is wrong with this idea, and what is right instead? “${proposal.text}”`}
                          />
                        )}
                      </article>
                    ))}
                  </div>
                </section>
              )}
            </div>

            <aside className="min-w-0 space-y-5">
              <FrozenSourceRail
                programId={programId}
                versionIds={session.summary.sourceVersionIds}
                disabledReason={
                  !sourceOpeningAllowed
                    ? 'Source opening is closed during an independent Demonstrate attempt.'
                    : busyLabel
                      ? 'Another workbench action is in progress.'
                      : undefined
                }
                onOpen={(src, verId) => void doOpenSource(src, verId, saveNow)}
              />
              {selectedSource && <ExactSourceReader query={sourceVersion} onClose={() => setSelectedSource(null)} />}
              <RubricPanel rubric={session.rubric} result={session.result} />
              <section className="rounded-2xl border border-border-default bg-surface p-5">
                <div className="text-xs font-semibold uppercase tracking-[.15em] text-text-muted">Assistance ledger</div>
                <h4 className="mt-1 font-serif text-lg text-text-primary">Help used in this attempt</h4>
                {session.assistance.length ? (
                  <ol className="mt-3 space-y-3">
                    {session.assistance.map((event) => (
                      <li key={event.id} className="flex gap-2 border-t border-border-default pt-3 first:border-0 first:pt-0">
                        <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent-muted text-accent">
                          <ArrowDownRight size={12} />
                        </span>
                        <span className="min-w-0">
                          <span className="block text-xs font-medium text-text-primary">{assistanceLabel(event.kind)}</span>
                          <span className="mt-0.5 block text-xs leading-4 text-text-muted">
                            {event.mode} · response revision {event.artifactRevision} · {dateLabel(event.createdAt)}
                          </span>
                        </span>
                      </li>
                    ))}
                  </ol>
                ) : (
                  <p className="mt-3 text-xs leading-5 text-text-muted">
                    No assistance recorded yet. Source openings, tutor turns, mode changes, and solution reveals are listed here.
                  </p>
                )}
                <div className="mt-4 rounded-lg bg-surface-sunken px-3 py-2 text-xs leading-4 text-text-muted">
                  <ShieldCheck size={12} className="mr-1 inline text-accent" /> Results are interpreted alongside the aid used.
                </div>
              </section>
              {solutionAllowed && !session.revealedSolution && (
                <section className="rounded-2xl border border-border-default bg-surface p-5">
                  <div className="flex items-center gap-2 text-accent">
                    <CircleHelp size={15} />
                    <h4 className="font-serif text-lg text-text-primary">Worked solution</h4>
                  </div>
                  <p className="mt-2 text-xs leading-5 text-text-secondary">
                    Revealing it records substantial assistance and adds it to your ledger. You can keep writing first.
                  </p>
                  <button
                    ref={solutionTriggerRef}
                    type="button"
                    disabled={Boolean(busyLabel)}
                    onClick={() => setSolutionConfirm(true)}
                    className="mt-3 rounded-full border border-accent px-4 py-2 text-xs font-semibold text-accent hover:bg-accent-muted"
                  >
                    Review reveal choice
                  </button>
                </section>
              )}
            </aside>
          </div>
        ) : null)}

      {solutionConfirm && (
        <ConfirmDialog
          title="Reveal the worked solution?"
          description="This will add substantial assistance to the attempt ledger. Your current response is saved first, and the solution will remain attached to this attempt."
          confirmLabel="Reveal solution"
          pending={Boolean(busyLabel)}
          onCancel={() => {
            setSolutionConfirm(false);
            window.requestAnimationFrame(() => solutionTriggerRef.current?.focus());
          }}
          onConfirm={() => void doReveal(saveNow)}
        />
      )}
    </section>
  );
}
