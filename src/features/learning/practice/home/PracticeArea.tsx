import { useCallback, useState } from 'react';

import { ArrowLeft } from 'lucide-react';

import { CourseDrillsPanel } from '@/features/learning/drills/CourseDrillsPanel';
import { useSaveLearningCardDraft } from '@/features/learning/memory/useLearningMemory';
import { CodingEntry } from '@/features/learning/practice/coding/CodingEntry';
import { CodingSession } from '@/features/learning/practice/coding/CodingSession';
import { FeynmanTechniquePanel } from '@/features/learning/practice/FeynmanTechniquePanel';
import { PracticalWorkbenchPanel } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import type { HistoryTarget } from './historyItems';
import { PracticeHome } from './PracticeHome';
import type { StartAction } from './practiceChoices';
import { practiceHomeApi } from './practiceHomeApi';
import { lastChosen } from './StartSentence';
import type { PracticeViewState } from './usePracticeView';

export interface PracticeAreaProps {
  program: Wire.LearningProgramDto;
  currentLesson: Wire.LearningLessonDto | undefined;
  currentModule: Wire.LearningModuleDto | undefined;
  view: PracticeViewState;
}

const SUB_TABS = [
  { id: 'attempt', label: 'Written practice' },
  { id: 'activity', label: 'Code & simulations' },
  { id: 'drill', label: 'Review drills' },
] as const;

/** The one Practice page and the screens it opens, with the work in each kept when the learner returns to the page. */
export function PracticeArea({ program, currentLesson, currentModule, view }: PracticeAreaProps) {
  const programId = program.summary.id;
  const saveRecallDraft = useSaveLearningCardDraft();
  const [startError, setStartError] = useState<string | null>(null);
  const kind = view.view.kind;
  const subView = kind === 'attempt' || kind === 'activity' || kind === 'drill';

  const start = useCallback(
    (action: StartAction) => {
      setStartError(null);
      if (action.kind === 'coding-build') view.openCodingBuild(action.choice);
      else if (action.kind === 'activity') view.openActivity(null, action.composer);
      else if (action.kind === 'attempt') view.openAttempt(action.mode);
      else view.openDrill();
    },
    [view],
  );

  const openTarget = useCallback(
    (target: HistoryTarget) => {
      setStartError(null);
      switch (target.kind) {
        case 'coding':
          view.openCoding(target.sessionId);
          break;
        case 'coding-activity':
          practiceHomeApi
            .startCodingSession(programId, target.activityId)
            .then(view.openCoding)
            .catch((error: unknown) => setStartError(getErrorMessage(error)));
          break;
        case 'activity':
          view.openActivity(target.activityId);
          break;
        case 'attempt':
          view.openAttempt();
          break;
        case 'drill':
          view.openDrill();
          break;
      }
    },
    [programId, view],
  );

  const startDue = useCallback(
    (item: Wire.DueItemDto) => {
      if (item.kind === 'coding_patterns') {
        view.openCodingBuild({
          lessonId: item.lessonId ?? currentLesson?.id ?? null,
          task: 'problem',
          setting: 'coached',
          language: lastChosen(programId, 'code', 'language') ?? 'python',
          difficulty: 'easy',
        });
      } else {
        view.openDrill();
      }
    },
    [currentLesson?.id, programId, view],
  );

  const tryInterview = useCallback(
    (setting: Wire.CodingSetting) => {
      view.openCodingBuild({
        lessonId: currentLesson?.id ?? null,
        task: 'problem',
        setting,
        language: lastChosen(programId, 'code', 'language') ?? 'python',
        difficulty: 'medium',
      });
    },
    [currentLesson?.id, programId, view],
  );

  const openSubTab = (id: (typeof SUB_TABS)[number]['id']) => {
    if (id === 'attempt') view.openAttempt(view.attemptMode);
    else if (id === 'drill') view.openDrill();
    else view.openActivity(view.activity.activityId, null);
  };

  return (
    <div className="space-y-4">
      {kind !== 'home' && (
        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={view.openHome}
            className="inline-flex min-h-11 cursor-pointer items-center gap-1.5 rounded-lg border border-border-default bg-surface px-3 text-sm font-semibold text-text-secondary hover:bg-surface-raised hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            <span>Back to Practice</span>
          </button>
          {subView && (
            <div role="tablist" aria-label="Practice activities" className="flex flex-wrap gap-1 rounded-lg border border-border-default bg-surface-sunken p-1">
              {SUB_TABS.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  role="tab"
                  aria-selected={kind === tab.id}
                  onClick={() => openSubTab(tab.id)}
                  className={`min-h-11 rounded-md px-3 text-sm font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                    kind === tab.id ? 'bg-surface text-text-primary shadow-xs' : 'text-text-secondary hover:text-text-primary'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {startError && <StatusLine state={{ kind: 'error', message: startError }} />}

      {view.view.kind === 'home' && (
        <PracticeHome
          programId={programId}
          goal={program.summary.goal}
          lessonId={currentLesson?.id ?? null}
          onStart={start}
          onOpenTarget={openTarget}
          onStartDue={startDue}
          onOpenWorkbench={() => view.openActivity()}
        />
      )}

      {view.view.kind === 'coding-build' && (
        <CodingEntry programId={programId} choice={view.view.choice} onReady={view.openCoding} onCancel={view.openHome} />
      )}
      {view.view.kind === 'coding' && (
        <CodingSession key={view.view.sessionId} programId={programId} sessionId={view.view.sessionId} onExit={view.openHome} onTryInterview={tryInterview} />
      )}

      {view.visited.has('attempt') && (
        <div hidden={kind !== 'attempt'} className="space-y-4">
          <div role="group" aria-label="Written practice mode" className="flex flex-wrap gap-1 rounded-lg border border-border-default bg-surface-sunken p-1">
            {(['practice', 'feynman'] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                aria-pressed={view.attemptMode === mode}
                onClick={() => view.openAttempt(mode)}
                className={`min-h-11 rounded-md px-3 text-sm font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                  view.attemptMode === mode ? 'bg-surface text-text-primary shadow-xs' : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                {mode === 'practice' ? 'Grounded practice' : 'Feynman technique (Teach a beginner)'}
              </button>
            ))}
          </div>
          <div hidden={view.attemptMode !== 'practice'}>
            <PracticeWorkbenchPanel key={currentLesson?.id} program={program} lesson={currentLesson} />
          </div>
          {view.attemptMode === 'feynman' && (
            <FeynmanTechniquePanel
              program={program}
              lesson={currentLesson}
              module={currentModule}
              onDraftMissedConcept={(question, answer) => {
                if (!currentLesson?.id) return;
                saveRecallDraft.mutate({
                  draftId: null,
                  programId,
                  lessonId: currentLesson.id,
                  question,
                  answer,
                  explanation: 'Drafted from a gap in a Feynman explanation',
                  sourceIds: [],
                });
              }}
            />
          )}
        </div>
      )}
      {view.visited.has('activity') && (
        <div hidden={kind !== 'activity'}>
          <PracticalWorkbenchPanel program={program} lesson={currentLesson ?? null} activityId={view.activity.activityId} composer={view.activity.composer} />
        </div>
      )}
      {view.visited.has('drill') && (
        <div hidden={kind !== 'drill'}>
          <CourseDrillsPanel programId={programId} />
        </div>
      )}
    </div>
  );
}
