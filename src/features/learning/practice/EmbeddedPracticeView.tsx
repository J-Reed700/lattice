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
import type {
  LearningLessonDto,
  LearningPracticeHintLevel,
  LearningProgramDto,
} from '@/lib/bindings';

export interface EmbeddedPracticeViewProps {
  program: LearningProgramDto;
  lesson: LearningLessonDto | undefined;
  sessionState: ReturnType<typeof usePracticeSession>;
  text: string;
  savedText: string;
  setText: (value: string) => void;
  textRef: React.MutableRefObject<string>;
  saveNow: () => Promise<boolean>;
  onContinue?: () => void;
  hints: { id: LearningPracticeHintLevel; title: string; prompt: string }[];
}

export function EmbeddedPracticeView({
  program,
  lesson,
  sessionState,
  text,
  savedText,
  setText,
  textRef,
  saveNow,
  onContinue,
  hints,
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
    busyLabel,
    actionError,
    conflict,
    retryAction,
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

  const nextHint = hints.slice(0, 3).find((hint) => !hintUsed.includes(hint.id));
  const busy = Boolean(busyLabel);
  const locked = busy && busyState !== 'saving';

  return (
    <div className="mt-5 space-y-4 border-t border-border-default pt-5">
      {!sessionId ? (
        <div>
          <p className="text-sm leading-6 text-text-secondary">
            Try the exercise in your own words. Your work saves as you go; request one hint at a time when you need it.
          </p>
          <button
            type="button"
            disabled={busy}
            onClick={() => void startSession()}
            className="mt-3 min-h-11 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg disabled:opacity-50"
          >
            {busy ? 'Starting…' : 'Start guided exercise'}
          </button>
        </div>
      ) : sessionQuery.isError ? (
        <StatusLine
          error={sessionQuery.error.message}
          onRetry={() => void sessionQuery.refetch()}
          retryLabel="Retry exercise"
          retryAriaLabel="Retry exercise"
        />
      ) : !session ? (
        <p role="status" className="text-xs text-text-muted">Opening your saved exercise…</p>
      ) : (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <label htmlFor={`guided-answer-${lessonId}`} className="text-sm font-semibold text-text-primary">
              Your working
            </label>
            <span role="status" className="text-xs text-text-muted">
              {busyLabel || (text === savedText ? 'Saved' : 'Saving…')}
            </span>
          </div>
          <textarea
            id={`guided-answer-${lessonId}`}
            rows={6}
            value={text}
            maxLength={LEARNER_TEXT_MAX_CHARS}
            disabled={!active || locked}
            onChange={(event) => {
              setText(event.target.value);
              textRef.current = event.target.value;
            }}
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
            </div>
          )}
          {session.tutorTurns.length > 0 && (
            <div aria-live="polite" className="space-y-3">
              {session.tutorTurns.map((turn, index) => (
                <article key={turn.id} className="rounded-xl bg-accent-muted p-4">
                  <p className="mb-2 text-xs font-semibold text-accent">
                    {turn.requestKind === 'hint'
                      ? `Hint · ${hints.find((hint) => hint.id === turn.hintLevel)?.title ?? index + 1}`
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
      {actionError && (
        <div role="alert" className="rounded-xl bg-danger-muted p-3 text-sm text-danger-fg">
          <p>{actionError}</p>
          {retryAction && (
            <button
              type="button"
              disabled={busy}
              onClick={() => void retryAction.run()}
              className="mt-2 font-semibold underline"
            >
              {retryAction.label}
            </button>
          )}
          {conflict && (
            <button
              type="button"
              onClick={() => void sessionQuery.refetch()}
              className="mt-2 font-semibold underline"
            >
              Reload saved exercise
            </button>
          )}
        </div>
      )}
    </div>
  );
}
