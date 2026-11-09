import { useRef } from 'react';

import { CircleHelp, Sparkles, Unlock } from 'lucide-react';

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
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import { usePracticeSession } from '@/features/learning/practice/usePracticeSession';
import { AssistanceLedger, dateLabel } from '@/features/learning/practice/workbench/AssistanceLedger';
import { HELP_MODES, helpModeOf } from '@/features/learning/practice/workbench/helpModes';
import { NewAttempt } from '@/features/learning/practice/workbench/NewAttempt';
import { ProposalList } from '@/features/learning/practice/workbench/ProposalList';
import { ResponseEditor } from '@/features/learning/practice/workbench/ResponseEditor';
import { TutorConversation } from '@/features/learning/practice/workbench/TutorConversation';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type {
  LearningLessonDto,
  LearningPracticeTaskKind,
  LearningProgramDto,
} from '@/lib/bindings';

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
    busy,
    actionError,
    conflict,
    retryAction,
    solutionConfirm,
    setSolutionConfirm,
    selectedSource,
    setSelectedSource,
    hintUsed,
    text,
    setText,
    dirty,
    autosave,
    saveNow,
    reloadAttempt,
    startSession,
    switchMode,
    sendTutor,
    doOpenSource,
    doReveal,
    doSubmit,
    decide,
    workspace,
  } = sessionState;

  const solutionTriggerRef = useRef<HTMLButtonElement>(null);
  const sourceRequest = selectedSource ? { programId, sourceId: selectedSource.sourceId, versionId: selectedSource.versionId } : null;
  const sourceVersion = useLearningSourceVersion(sourceRequest);

  const sessionList = workspace.data?.sessions ?? [];
  if (workspace.isLoading) {
    return <div role="status" className="rounded-2xl border border-border-default bg-surface p-8 text-xs text-text-muted">Opening your practice workbench…</div>;
  }
  if (workspace.isError) {
    return (
      <div className="rounded-2xl border border-border-default bg-surface p-6">
        <StatusLine state={{ kind: 'error', message: workspace.error.message, retry: () => void workspace.refetch() }} />
      </div>
    );
  }

  if (embedded) {
    return <EmbeddedPracticeView program={program} lesson={lesson} sessionState={sessionState} onContinue={onContinue} />;
  }

  const openCitation = (citation: { sourceId: string; versionId: string }) => {
    const src = sourceForVersion(sourceWorkspace.data?.sources ?? [], citation.sourceId, citation.versionId);
    if (src) void doOpenSource(src, citation.versionId, saveNow);
  };

  return (
    <section className="min-w-0 space-y-5" aria-label="Grounded Practice Workbench">
      <header className="rounded-2xl border border-border-default bg-surface p-5 sm:p-7">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.16em] text-accent">
              <Sparkles size={14} aria-hidden="true" /> Grounded Practice Workbench
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
          <NewAttempt
            choice={modeChoice}
            onChoose={setModeChoice}
            onStart={() => void startSession()}
            starting={busyState === 'starting'}
            busy={busy}
          />
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
              <div className="flex flex-col items-end gap-1">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void startSession()}
                  className="rounded-full border border-accent px-4 py-2 text-xs font-semibold text-accent hover:bg-accent-muted disabled:opacity-50"
                >
                  Revise in a new attempt
                </button>
                {busy && <p className="text-xs text-text-muted">Another action is in progress.</p>}
              </div>
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
            <StatusLine
              state={{ kind: 'error', message: `Could not load attempt: ${sessionQuery.error.message}`, retry: () => void sessionQuery.refetch() }}
            />
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
                  {HELP_MODES.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      aria-pressed={mode === item.id}
                      disabled={!active || busy}
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
                  Aid contract: {helpModeOf(mode)?.description}
                </p>
              </section>

              <ResponseEditor
                text={text}
                dirty={dirty}
                active={active}
                autosave={autosave}
                actionError={actionError}
                onRetryAction={retryAction ? () => void retryAction.run() : undefined}
                conflict={conflict}
                busy={busy}
                submitting={busyState === 'submitting'}
                onChange={setText}
                onReload={() => void reloadAttempt()}
                onSubmit={() => void doSubmit(text, saveNow)}
              />

              {session.result && <ResultPanel result={session.result} />}

              {session.summary.revisesSessionId && <PreviousFeedback session={session} />}

              <TutorConversation
                sessionId={session.summary.id}
                turns={session.tutorTurns}
                mode={mode}
                active={active}
                canTutor={canTutor}
                busyState={busyState}
                busy={busy}
                hintUsed={hintUsed}
                showCitations={showCitations}
                sourceOpeningAllowed={sourceOpeningAllowed}
                onAsk={(kind, prompt) => sendTutor(kind, prompt, undefined, saveNow)}
                onHint={(hint) => sendTutor('hint', hint.prompt, hint.id, saveNow)}
                onOpenCitation={openCitation}
              />

              {session.revealedSolution && (
                <section className="rounded-2xl border border-border-default bg-surface p-5">
                  <div className="flex items-center gap-2 text-accent">
                    <Unlock size={15} aria-hidden="true" />
                    <h4 className="font-serif text-xl text-text-primary">Worked solution revealed</h4>
                  </div>
                  <div className="mt-3 text-sm leading-7 text-text-secondary">
                    <TiptapViewer content={session.revealedSolution} />
                  </div>
                  {showCitations && session.revealedSolutionCitations.length > 0 && (
                    <CitationCards
                      disabled={!sourceOpeningAllowed || busy}
                      citations={session.revealedSolutionCitations}
                      onOpen={openCitation}
                    />
                  )}
                </section>
              )}

              <ProposalList
                programId={programId}
                proposals={session.proposals}
                active={active}
                busy={busy}
                onDecide={(proposalId, accepted) => void decide(proposalId, accepted, saveNow)}
              />
            </div>

            <aside className="min-w-0 space-y-5">
              <FrozenSourceRail
                programId={programId}
                versionIds={session.summary.sourceVersionIds}
                disabledReason={
                  !sourceOpeningAllowed
                    ? 'Source opening is closed during an independent Demonstrate attempt.'
                    : busy
                      ? 'Another workbench action is in progress.'
                      : undefined
                }
                onOpen={(src, verId) => void doOpenSource(src, verId, saveNow)}
              />
              {selectedSource && <ExactSourceReader query={sourceVersion} onClose={() => setSelectedSource(null)} />}
              <RubricPanel rubric={session.rubric} result={session.result} />
              <AssistanceLedger events={session.assistance} />
              {solutionAllowed && !session.revealedSolution && (
                <section className="rounded-2xl border border-border-default bg-surface p-5">
                  <div className="flex items-center gap-2 text-accent">
                    <CircleHelp size={15} aria-hidden="true" />
                    <h4 className="font-serif text-lg text-text-primary">Worked solution</h4>
                  </div>
                  <p className="mt-2 text-xs leading-5 text-text-secondary">
                    Revealing it records substantial assistance and adds it to your ledger. You can keep writing first.
                  </p>
                  <button
                    ref={solutionTriggerRef}
                    type="button"
                    disabled={busy}
                    onClick={() => setSolutionConfirm(true)}
                    className="mt-3 rounded-full border border-accent px-4 py-2 text-xs font-semibold text-accent hover:bg-accent-muted disabled:opacity-50"
                  >
                    Review reveal choice
                  </button>
                  {busy && <p className="mt-1 text-xs text-text-muted">Another action is in progress.</p>}
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
          pending={busy}
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
