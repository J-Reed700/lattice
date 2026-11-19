import { useEffect, useRef } from 'react';

import { useBlocker } from 'react-router';

/** Keep the editor mounted until the repository acknowledges its draft. */
export function useJournalNavigationGuard(dirty: boolean, saveNow: () => Promise<boolean>) {
  const blocker = useBlocker(dirty);
  const blockerRef = useRef(blocker);
  blockerRef.current = blocker;
  const mountedRef = useRef(true);
  const saving = useRef(false);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (blocker.state !== 'blocked' || saving.current) return;
    saving.current = true;
    void saveNow()
      .then(
        (saved) => {
          const current = blockerRef.current;
          if (!mountedRef.current || current.state !== 'blocked') return;
          if (saved) current.proceed();
          else current.reset();
        },
        () => {
          const current = blockerRef.current;
          if (!mountedRef.current || current.state !== 'blocked') return;
          current.reset();
        },
      )
      .catch(() => {
        // Guard against unexpected errors thrown by blocker state transitions.
      })
      .finally(() => {
        saving.current = false;
      });
  }, [blocker, saveNow]);
}
