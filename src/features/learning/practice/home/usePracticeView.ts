import { useCallback, useMemo, useState } from 'react';

import type { CodingBuildChoice } from '@/features/learning/practice/coding/CodingEntry';
import type { LearningInterviewRoundKind, LearningPracticalActivityKind } from '@/lib/bindings';

/** A request to open the activity composer already set to a kind, brief or interview. */
export interface ComposerRequest {
  id: string;
  kind: LearningPracticalActivityKind;
  brief?: string;
  interview?: {
    companyName: string | null;
    targetRole: string | null;
    roundKind: LearningInterviewRoundKind;
  };
}

export type AttemptMode = 'practice' | 'feynman';

export type PracticeView =
  | { kind: 'home' }
  | { kind: 'coding-build'; choice: CodingBuildChoice }
  | { kind: 'coding'; sessionId: string }
  | { kind: 'attempt'; mode: AttemptMode }
  | { kind: 'drill' }
  | { kind: 'activity'; activityId: string | null; composer: ComposerRequest | null };

/** Views whose screens stay mounted, hidden, once opened, so their work is still there on return. */
export type RetainedKind = 'attempt' | 'drill' | 'activity';

export interface PracticeViewState {
  view: PracticeView;
  visited: ReadonlySet<RetainedKind>;
  /** The mode of the last written-practice screen opened. */
  attemptMode: AttemptMode;
  /** The activity and composer request of the last practical screen opened. */
  activity: { activityId: string | null; composer: ComposerRequest | null };
  isHome: boolean;
  openHome: () => void;
  openCodingBuild: (choice: CodingBuildChoice) => void;
  openCoding: (sessionId: string) => void;
  openAttempt: (mode?: AttemptMode) => void;
  openDrill: () => void;
  openActivity: (activityId?: string | null, composer?: ComposerRequest | null) => void;
  /** Home, and every retained screen discarded (after an import replaced the course's work). */
  reset: () => void;
}

interface State {
  view: PracticeView;
  visited: ReadonlySet<RetainedKind>;
  attemptMode: AttemptMode;
  activity: { activityId: string | null; composer: ComposerRequest | null };
}

const NO_ACTIVITY = { activityId: null, composer: null } as const;

function retained(view: PracticeView): RetainedKind | null {
  return view.kind === 'attempt' || view.kind === 'drill' || view.kind === 'activity' ? view.kind : null;
}

/** Which screen of the Practice area is open. */
export function usePracticeView(initial: PracticeView = { kind: 'home' }): PracticeViewState {
  const [state, setState] = useState<State>(() => {
    const kind = retained(initial);
    return {
      view: initial,
      visited: new Set(kind ? [kind] : []),
      attemptMode: initial.kind === 'attempt' ? initial.mode : 'practice',
      activity: initial.kind === 'activity' ? { activityId: initial.activityId, composer: initial.composer } : NO_ACTIVITY,
    };
  });

  const show = useCallback((view: PracticeView) => {
    setState((current) => {
      const kind = retained(view);
      const visited = kind && !current.visited.has(kind) ? new Set([...current.visited, kind]) : current.visited;
      return {
        view,
        visited,
        attemptMode: view.kind === 'attempt' ? view.mode : current.attemptMode,
        activity: view.kind === 'activity' ? { activityId: view.activityId, composer: view.composer } : current.activity,
      };
    });
  }, []);

  const openHome = useCallback(() => show({ kind: 'home' }), [show]);
  const openCodingBuild = useCallback((choice: CodingBuildChoice) => show({ kind: 'coding-build', choice }), [show]);
  const openCoding = useCallback((sessionId: string) => show({ kind: 'coding', sessionId }), [show]);
  const openAttempt = useCallback((mode: AttemptMode = 'practice') => show({ kind: 'attempt', mode }), [show]);
  const openDrill = useCallback(() => show({ kind: 'drill' }), [show]);
  const openActivity = useCallback(
    (activityId: string | null = null, composer: ComposerRequest | null = null) =>
      show({ kind: 'activity', activityId, composer }),
    [show],
  );
  const reset = useCallback(() => setState({ view: { kind: 'home' }, visited: new Set(), attemptMode: 'practice', activity: NO_ACTIVITY }), []);

  return useMemo(
    () => ({
      view: state.view,
      visited: state.visited,
      attemptMode: state.attemptMode,
      activity: state.activity,
      isHome: state.view.kind === 'home',
      openHome,
      openCodingBuild,
      openCoding,
      openAttempt,
      openDrill,
      openActivity,
      reset,
    }),
    [state, openHome, openCodingBuild, openCoding, openAttempt, openDrill, openActivity, reset],
  );
}
