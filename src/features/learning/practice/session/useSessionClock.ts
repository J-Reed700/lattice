import { useEffect, useMemo, useState } from 'react';

import type * as Wire from '@/lib/bindings';

import { sessionApi } from './sessionApi';

/** The session's clock: open while the page is visible, closed when it hides or the screen leaves an unfinished session. */
export function useSessionClock(
  programId: string,
  initial: Wire.PracticeSessionDto,
  isFinished: () => boolean,
) {
  const { sessionId } = initial;
  const [clock, setClock] = useState(initial.clock);

  useEffect(() => {
    if (initial.finished) return;
    const set = (open: boolean) => {
      sessionApi
        .setClock({ programId, sessionId, open })
        .then(setClock)
        .catch(() => undefined);
    };
    if (!document.hidden) set(true);
    const onVisibility = () => set(!document.hidden);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      if (!isFinished()) set(false);
    };
  }, [initial.finished, isFinished, programId, sessionId]);

  const elapsedMs = useMemo(
    () => clock.activeMs + (clock.openSince != null ? Math.max(0, Date.now() - clock.openSince) : 0),
    [clock],
  );

  return { clock, elapsedMs };
}
