import { SaveStatus } from '@/components/ui/SaveStatus';
import { TiptapViewer } from '@/components/TiptapEditor';
import { LEARNER_TEXT_MAX_CHARS } from '@/features/learning/model/learnerTextLimit';
import {
  PreviousFeedback,
  RubricPanel,
} from '@/features/learning/practice/PracticeRubricPanel';
import {
  CitationCards,
  ExactSourceReader,
  sourceForVersion,
} from '@/features/learning/practice/PracticeSourceRail';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import type { usePracticeSession } from '@/features/learning/practice/usePracticeSession';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { HINTS } from '@/features/learning/practice/workbench/helpModes';
import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

export interface EmbeddedPracticeViewProps {
  program: LearningProgramDto;
  lesson: LearningLessonDto | undefined;
  sessionState: ReturnType<typeof usePracticeSession>;
  onContinue?: () => void;
}

export function EmbeddedPracticeView({
  program,
  lesson,
  sessionState,
  onContinue,
}: EmbeddedPracticeViewProps) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const programId = program.summary.id;
  const lessonId = lesson?.id ?? '';
  const sourceWorkspace = useLearningSourceWorkspace(programId);

  const {
    sessionId,
    sessionQuery,
    session,
    active,
    busyState,
    busy,
    text,
    setText,
    dirty,
    autosave,
    saveNow,
    reloadAttempt,
    failure,
    conflict,
    selectedSource,
    setSelectedSource,
    hintUsed,
    sourceOpeningAllowed,
    startSession,
    sendTutor,
    doOpenSource,
    doSubmit,
  } = sessionState;

  const sourceRequest = selectedSource
    ? { programId, sourceId: selectedSource.sourceId, versionId: selectedSource.versionId }
    : null;
  const sourceVersion = useLearningSourceVersion(sourceRequest);

  const nextHint = HINTS.slice(0, 3).find((hint) => !hintUsed.includes(hint.id));
  const saveState = autosave.state === 'saving' ? 'saving' : dirty || autosave.state === 'error' || autosave.state === 'conflict' ? 'unsaved' : 'saved';
  const saveFailed = autosave.state === 'error' || autosave.state === 'conflict';
  const error = failure
    ? { message: failure.message, retry: failure.retry }
    : saveFailed
      ? { message: autosave.error ?? 'Your work could not be saved.', retry: autosave.state === 'error' ? () => void autosave.flush() : undefined }
      : null;
  const submitReason = !text.trim() ? 'Write your working first.' : busy && busyState !== 'submitting' ? 'Another action is in progress.' : null;

  return (
    <section aria-label="Guided attempt" className="mt-5 space-y-4 border-t border-border-default pt-5">
      {!sessionId ? (
        <div>
          <p className="text-sm leading-6 text-text-secondary">
            Try the attempt in your own words. Your work saves as you go; request one hint at a time when you need it.
          </p>
          <button
            type="button"
            disabled={busy}
            onClick={() => void startSession()}
            className="mt-3 min-h-11 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg disabled:opacity-50"
          >
            {busyState === 'starting' ? 'Starting…' : 'Start guided attempt'}
          </button>
        </div>
      ) : sessionQuery.isError ? (
        <StatusLine state={{ kind: 'error', message: sessionQuery.error.message, retry: () => void sessionQuery.refetch() }} />
      ) : !session ? (
        <p role="status" className="text-xs text-text-muted">Opening your saved attempt…</p>
      ) : (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <label htmlFor={`guided-answer-${lessonId}`} className="text-sm font-semibold text-text-primary">
              Your working
            </label>
            <SaveStatus state={saveState} />
          </div>
          <textarea
            id={`guided-answer-${lessonId}`}
            rows={6}
            value={text}
            maxLength={LEARNER_TEXT_MAX_CHARS}
            disabled={!active || busy}
            onChange={(event) => setText(event.target.value)}
            placeholder="Show your first step, explain your reasoning, or paste your code…"
            className="w-full rounded-xl border border-border-default bg-surface p-4 text-sm leading-6 text-text-primary outline-hidden focus:border-accent disabled:opacity-70"
          />
          {active && (
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={busy || !text.trim()}
                onClick={() =>
                  void sendTutor(
                    'critique',
                    'Check my current reasoning against the exercise. Identify the first specific gap and ask a useful question. Do not reveal the final answer.',
                    undefined,
                    saveNow,
                  )
                }
                className="min-h-11 rounded-full border border-accent px-4 text-sm font-medium text-accent hover:bg-accent-muted disabled:opacity-50"
              >
                Check my reasoning
              </button>
              {nextHint && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void sendTutor('hint', nextHint.prompt, nextHint.id, saveNow)}
                  className="min-h-11 rounded-full border border-border-default px-4 text-sm font-medium text-text-primary hover:border-accent hover:text-accent disabled:opacity-50"
                >
                  {hintUsed.length ? 'Next hint' : 'Give me a hint'}
                </button>
              )}
              <button
                type="button"
                disabled={busy || !text.trim()}
                onClick={() => void doSubmit(text, saveNow)}
                className="min-h-11 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg disabled:opacity-50"
              >
                Submit guided response
              </button>
              {submitReason && <p className="w-full text-xs text-text-muted">{submitReason}</p>}
            </div>
          )}
          {session.tutorTurns.length > 0 && (
            <div aria-live="polite" className="space-y-3">
              {session.tutorTurns.map((turn, index) => (
                <article key={turn.id} className="rounded-xl bg-accent-muted p-4">
                  <p className="mb-2 text-xs font-semibold text-accent">
                    {turn.requestKind === 'hint'
                      ? `Hint · ${HINTS.find((hint) => hint.id === turn.hintLevel)?.title ?? index + 1}`
                      : 'Feedback on your reasoning'}
                  </p>
                  <div className="text-sm leading-6 text-text-primary">
                    <TiptapViewer content={turn.response} />
                  </div>
                  {showCitations && turn.citations.length > 0 && (
                    <CitationCards
                      disabled={!sourceOpeningAllowed || busy}
                      citations={turn.citations}
                      onOpen={(citation) => {
                        const src = sourceForVersion(
                          sourceWorkspace.data?.sources ?? [],
                          citation.sourceId,
                          citation.versionId,
                        );
                        if (src) void doOpenSource(src, citation.versionId, saveNow);
                      }}
                    />
                  )}
                </article>
              ))}
            </div>
          )}
          {showCitations && selectedSource && (
            <ExactSourceReader query={sourceVersion} onClose={() => setSelectedSource(null)} />
          )}
          <details open={Boolean(session.result)} className="rounded-xl border border-border-default bg-surface">
            <summary className="cursor-pointer p-3 text-sm font-medium text-text-primary">
              {session.result ? 'Your feedback and next revision' : 'What good work looks like'}
            </summary>
            <RubricPanel rubric={session.rubric} result={session.result} />
          </details>
          {session.result && (
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() => void startSession()}
                className="min-h-11 rounded-full border border-accent px-4 text-sm font-medium text-accent hover:bg-accent-muted"
              >
                Revise this response
              </button>
              {onContinue && (
                <button
                  type="button"
                  onClick={onContinue}
                  className="min-h-11 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg"
                >
                  Continue to independent assignment
                </button>
              )}
            </div>
          )}
          {session.summary.revisesSessionId && <PreviousFeedback session={session} />}
        </>
      )}
      {error && <StatusLine state={{ kind: 'error', message: error.message, retry: error.retry }} />}
      {conflict && (
        <button type="button" onClick={() => void reloadAttempt()} className="text-sm font-semibold underline">
          Reload the saved attempt · keep my text
        </button>
      )}
    </section>
  );
}
