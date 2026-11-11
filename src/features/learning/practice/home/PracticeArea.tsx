import { useCallback, useId, useRef, useState, type KeyboardEvent } from 'react';

import { ArrowLeft } from 'lucide-react';

import { CourseDrillsPanel } from '@/features/learning/drills/CourseDrillsPanel';
import { FeynmanTechniquePanel } from '@/features/learning/practice/FeynmanTechniquePanel';
import { codeKind, kinds } from '@/features/learning/practice/kinds';
import { PracticalWorkbenchPanel } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { PracticeSession } from '@/features/learning/practice/session/PracticeSession';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import { CODING_LANGUAGE, MOCK_DIFFICULTY, type StartAction } from './practiceChoices';
import { PracticeHome } from './PracticeHome';
import { practiceHomeApi } from './practiceHomeApi';

import type { HistoryTarget } from './historyItems';
import type { PracticeViewState } from './usePracticeView';

export interface PracticeAreaProps {
  program: Wire.LearningProgramDto;
  currentLesson: Wire.LearningLessonDto | undefined;
  currentModule: Wire.LearningModuleDto | undefined;
  view: PracticeViewState;
}

const { Entry: CodeEntry } = codeKind;

const SUB_TABS = [
  { id: 'attempt', label: 'Written practice' },
  { id: 'activity', label: 'Code & simulations' },
  { id: 'drill', label: 'Review drills' },
] as const;

type SubTabId = (typeof SUB_TABS)[number]['id'];

/** The one Practice page and the screens it opens, with the work in each kept when the learner returns to the page. */
export function PracticeArea({ program, currentLesson, currentModule, view }: PracticeAreaProps) {
  const programId = program.summary.id;
  const uid = useId();
  const tabRefs = useRef<Partial<Record<SubTabId, HTMLButtonElement | null>>>({});
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
            .startPracticeSession(programId, target.activityId)
            .then(view.openCoding)
            .catch((error: unknown) => setStartError(getErrorMessage(error)));
          break;
        case 'activity':
          view.openActivity(target.activityId);
          break;
        case 'attempt':
          view.openAttempt(target.mode, { sessionId: target.sessionId, lessonId: target.lessonId });
          break;
        case 'drill':
          view.openDrill(target.attemptId);
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
          form: 'build',
          setting: 'coached',
          language: CODING_LANGUAGE,
          difficulty: 'easy',
        });
      } else {
        view.openDrill();
      }
    },
    [currentLesson?.id, view],
  );

  const tryInterview = useCallback(
    (setting: Wire.Setting) => {
      view.openCodingBuild({
        lessonId: currentLesson?.id ?? null,
        form: 'build',
        setting,
        language: CODING_LANGUAGE,
        difficulty: MOCK_DIFFICULTY,
      });
    },
    [currentLesson?.id, view],
  );

  const openSubTab = (id: SubTabId) => {
    if (id === 'attempt') view.openAttempt(view.attemptMode, view.attempt);
    else if (id === 'drill') view.openDrill(view.drillAttemptId);
    else view.openActivity(view.activity.activityId, null);
  };

  const onTabKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = SUB_TABS.findIndex((tab) => tab.id === kind);
    const last = SUB_TABS.length - 1;
    const next =
      event.key === 'ArrowRight' ? (index + 1) % SUB_TABS.length
      : event.key === 'ArrowLeft' ? (index + last) % SUB_TABS.length
      : event.key === 'Home' ? 0
      : event.key === 'End' ? last
      : null;
    if (next === null) return;
    event.preventDefault();
    const target = SUB_TABS[next]!.id;
    openSubTab(target);
    tabRefs.current[target]?.focus();
  };

  const tabId = (id: SubTabId) => `${uid}-tab-${id}`;
  const panelId = (id: SubTabId) => `${uid}-panel-${id}`;
  const attemptLesson = view.attempt
    ? (program.modules.flatMap((module) => module.lessons).find((lesson) => lesson.id === view.attempt?.lessonId) ?? currentLesson)
    : currentLesson;

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
            <div role="tablist" aria-label="Practice activities" onKeyDown={onTabKeyDown} className="flex flex-wrap gap-1 rounded-lg border border-border-default bg-surface-sunken p-1">
              {SUB_TABS.map((tab) => (
                <button
                  key={tab.id}
                  ref={(node) => {
                    tabRefs.current[tab.id] = node;
                  }}
                  id={tabId(tab.id)}
                  type="button"
                  role="tab"
                  aria-selected={kind === tab.id}
                  aria-controls={panelId(tab.id)}
                  tabIndex={kind === tab.id ? 0 : -1}
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
        <CodeEntry programId={programId} choice={view.view.choice} onReady={view.openCoding} onCancel={view.openHome} />
      )}
      {view.view.kind === 'coding' && (
        <PracticeSession key={view.view.sessionId} programId={programId} sessionId={view.view.sessionId} kinds={kinds} onExit={view.openHome} onTryInterview={tryInterview} />
      )}

      <div role="tabpanel" id={panelId('attempt')} aria-labelledby={tabId('attempt')} hidden={kind !== 'attempt'} className="space-y-4">
        {view.visited.has('attempt') && (
          <>
            <div role="group" aria-label="Written practice mode" className="flex flex-wrap gap-1 rounded-lg border border-border-default bg-surface-sunken p-1">
              {(['practice', 'feynman'] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  aria-pressed={view.attemptMode === mode}
                  onClick={() => view.openAttempt(mode, view.attempt)}
                  className={`min-h-11 rounded-md px-3 text-sm font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                    view.attemptMode === mode ? 'bg-surface text-text-primary shadow-xs' : 'text-text-secondary hover:text-text-primary'
                  }`}
                >
                  {mode === 'practice' ? 'Grounded practice' : 'Feynman technique (Teach a beginner)'}
                </button>
              ))}
            </div>
            <div hidden={view.attemptMode !== 'practice'}>
              <PracticeWorkbenchPanel
                key={`${attemptLesson?.id ?? ''}:${view.attempt?.sessionId ?? ''}`}
                program={program}
                lesson={attemptLesson}
                attemptId={view.attempt?.sessionId ?? null}
              />
            </div>
            {view.attemptMode === 'feynman' && <FeynmanTechniquePanel program={program} lesson={currentLesson} module={currentModule} />}
          </>
        )}
      </div>
      <div role="tabpanel" id={panelId('activity')} aria-labelledby={tabId('activity')} hidden={kind !== 'activity'}>
        {view.visited.has('activity') && (
          <PracticalWorkbenchPanel program={program} lesson={currentLesson ?? null} activityId={view.activity.activityId} composer={view.activity.composer} />
        )}
      </div>
      <div role="tabpanel" id={panelId('drill')} aria-labelledby={tabId('drill')} hidden={kind !== 'drill'}>
        {view.visited.has('drill') && <CourseDrillsPanel programId={programId} attemptId={view.drillAttemptId} />}
      </div>
    </div>
  );
}
