import { useEffect, useMemo, useState, type RefObject } from 'react';

import type * as Wire from '@/lib/bindings';

import { sessionApi } from './sessionApi';

/** The session's clock: open while the page is visible, closed when it hides or the screen leaves. */
export function useSessionClock(
  programId: string,
  initial: Wire.PracticeSessionDto,
  sessionRef: RefObject<Wire.PracticeSessionDto>,
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
      if (!sessionRef.current.finished) set(false);
    };
  }, [initial.finished, programId, sessionId, sessionRef]);

  const elapsedMs = useMemo(
    () => clock.activeMs + (clock.openSince != null ? Math.max(0, Date.now() - clock.openSince) : 0),
    [clock],
  );

  return { clock, elapsedMs };
}
