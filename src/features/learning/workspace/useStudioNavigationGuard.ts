import { useEffect, useRef, useState } from 'react';

import { useBlocker } from 'react-router';

import { flushPendingSaves } from '@/lib/pendingSaves';

/** Sidebar, history, and keyboard navigation must wait for acknowledged drafts. */
export function useStudioNavigationGuard(enabled: boolean) {
  const blocker = useBlocker(enabled);
  const blockerRef = useRef(blocker);
  blockerRef.current = blocker;
  const mountedRef = useRef(true);
  const saving = useRef(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (blocker.state !== 'blocked' || saving.current) return;
    saving.current = true;
    setError(null);
    void flushPendingSaves()
      .then((saved) => {
        const current = blockerRef.current;
        if (!mountedRef.current || current.state !== 'blocked') return;
        if (saved) {
          current.proceed();
        } else {
          setError('Your work could not be saved. You are still in Studio; retry saving before leaving.');
          current.reset();
        }
      })
      .catch(() => {
        const current = blockerRef.current;
        if (!mountedRef.current) return;
        setError('Your work could not be saved. You are still in Studio; retry saving before leaving.');
        if (current.state === 'blocked') {
          current.reset();
        }
      })
      .finally(() => {
        saving.current = false;
      });
  }, [blocker]);

  return { error };
}
