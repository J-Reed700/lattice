import { useCallback, useState } from 'react';

export type PracticeViewKind = 'home' | 'session' | 'attempt' | 'drill';

export interface PracticeSessionTarget {
  sessionId?: string;
  activityId?: string;
}

export interface PracticeAttemptTarget {
  attemptId?: string;
  lessonId?: string;
}

export interface PracticeDrillTarget {
  drillId?: string;
  attemptId?: string;
  lessonId?: string;
}

export type PracticeView =
  | { kind: 'home' }
  | ({ kind: 'session' } & PracticeSessionTarget)
  | ({ kind: 'attempt' } & PracticeAttemptTarget)
  | ({ kind: 'drill' } & PracticeDrillTarget);

export interface UsePracticeViewReturn {
  view: PracticeView;
  isHome: boolean;
  openHome: () => void;
  openSession: (target?: PracticeSessionTarget | string) => void;
  openAttempt: (target?: PracticeAttemptTarget | string) => void;
  openDrill: (target?: PracticeDrillTarget | string) => void;
  setView: (view: PracticeView) => void;
}

/** Hook to manage the active view inside the unified Practice area. Minimal non-panicking stub for Stage 1. */
export function usePracticeView(
  initialView: PracticeView = { kind: 'home' },
): UsePracticeViewReturn {
  const [view, setView] = useState<PracticeView>(initialView);

  const openHome = useCallback(() => {
    setView({ kind: 'home' });
  }, []);

  const openSession = useCallback((target?: PracticeSessionTarget | string) => {
    const payload = typeof target === 'string' ? { sessionId: target } : (target ?? {});
    setView({ kind: 'session', ...payload });
  }, []);

  const openAttempt = useCallback((target?: PracticeAttemptTarget | string) => {
    const payload = typeof target === 'string' ? { attemptId: target } : (target ?? {});
    setView({ kind: 'attempt', ...payload });
  }, []);

  const openDrill = useCallback((target?: PracticeDrillTarget | string) => {
    const payload = typeof target === 'string' ? { drillId: target } : (target ?? {});
    setView({ kind: 'drill', ...payload });
  }, []);

  return {
    view,
    isHome: view.kind === 'home',
    openHome,
    openSession,
    openAttempt,
    openDrill,
    setView,
  };
}
