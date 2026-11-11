import { useCallback, useMemo, useState } from 'react';

import type { ComposerRequest } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import type { CodingBuildChoice } from '@/features/learning/practice/session/kind';

export type AttemptMode = 'practice' | 'feynman';

/** A written attempt to reopen, in the lesson it was made on. */
export interface AttemptRef {
  sessionId: string;
  lessonId: string;
}

export type PracticeView =
  | { kind: 'home' }
  | { kind: 'coding-build'; choice: CodingBuildChoice }
  | { kind: 'coding'; sessionId: string }
  | { kind: 'attempt'; mode: AttemptMode; attempt: AttemptRef | null }
  | { kind: 'drill'; attemptId: string | null }
  | { kind: 'activity'; activityId: string | null; composer: ComposerRequest | null };

/** Views whose screens stay mounted, hidden, once opened, so their work is still there on return. */
export type RetainedKind = 'attempt' | 'drill' | 'activity';

export interface PracticeViewState {
  view: PracticeView;
  visited: ReadonlySet<RetainedKind>;
  /** What each retained screen was last opened on. */
  attemptMode: AttemptMode;
  attempt: AttemptRef | null;
  drillAttemptId: string | null;
  activity: { activityId: string | null; composer: ComposerRequest | null };
  isHome: boolean;
  openHome: () => void;
  openCodingBuild: (choice: CodingBuildChoice) => void;
  openCoding: (sessionId: string) => void;
  openAttempt: (mode?: AttemptMode, attempt?: AttemptRef | null) => void;
  openDrill: (attemptId?: string | null) => void;
  openActivity: (activityId?: string | null, composer?: ComposerRequest | null) => void;
  /** Home, and every retained screen discarded (after an import replaced the course's work). */
  reset: () => void;
}

interface State {
  view: PracticeView;
  visited: ReadonlySet<RetainedKind>;
  attemptMode: AttemptMode;
  attempt: AttemptRef | null;
  drillAttemptId: string | null;
  activity: { activityId: string | null; composer: ComposerRequest | null };
}

const NO_ACTIVITY = { activityId: null, composer: null } as const;

function retained(view: PracticeView): RetainedKind | null {
  return view.kind === 'attempt' || view.kind === 'drill' || view.kind === 'activity' ? view.kind : null;
}

function stateOf(view: PracticeView, previous: State | null): State {
  const kind = retained(view);
  const visited = previous ? new Set(previous.visited) : new Set<RetainedKind>();
  if (kind) visited.add(kind);
  return {
    view,
    visited,
    attemptMode: view.kind === 'attempt' ? view.mode : (previous?.attemptMode ?? 'practice'),
    attempt: view.kind === 'attempt' ? view.attempt : (previous?.attempt ?? null),
    drillAttemptId: view.kind === 'drill' ? view.attemptId : (previous?.drillAttemptId ?? null),
    activity: view.kind === 'activity' ? { activityId: view.activityId, composer: view.composer } : (previous?.activity ?? NO_ACTIVITY),
  };
}

/** Which screen of the Practice area is open. */
export function usePracticeView(initial: PracticeView = { kind: 'home' }): PracticeViewState {
  const [state, setState] = useState<State>(() => stateOf(initial, null));

  const show = useCallback((view: PracticeView) => setState((current) => stateOf(view, current)), []);

  const openHome = useCallback(() => show({ kind: 'home' }), [show]);
  const openCodingBuild = useCallback((choice: CodingBuildChoice) => show({ kind: 'coding-build', choice }), [show]);
  const openCoding = useCallback((sessionId: string) => show({ kind: 'coding', sessionId }), [show]);
  const openAttempt = useCallback(
    (mode: AttemptMode = 'practice', attempt: AttemptRef | null = null) => show({ kind: 'attempt', mode, attempt }),
    [show],
  );
  const openDrill = useCallback((attemptId: string | null = null) => show({ kind: 'drill', attemptId }), [show]);
  const openActivity = useCallback(
    (activityId: string | null = null, composer: ComposerRequest | null = null) => show({ kind: 'activity', activityId, composer }),
    [show],
  );
  const reset = useCallback(() => setState(stateOf({ kind: 'home' }, null)), []);

  return useMemo(
    () => ({
      view: state.view,
      visited: state.visited,
      attemptMode: state.attemptMode,
      attempt: state.attempt,
      drillAttemptId: state.drillAttemptId,
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
