import { useCallback, useRef, useState } from 'react';

import { isConflict } from '@/lib/api';
import { getErrorMessage } from '@/lib/errorUtils';

/**
 * Runs an action that can fail, keeping its error text and the arguments of its
 * last run so Try again repeats it exactly. A run while one is going does nothing,
 * and after a conflict the stale request is not repeated: the caller reloads.
 */
export function useRetry<A extends unknown[], R>(action: (...args: A) => Promise<R>) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const actionRef = useRef(action);
  actionRef.current = action;
  const running = useRef(false);
  const lastArgs = useRef<A | null>(null);

  const run = useCallback(async (...args: A): Promise<R | undefined> => {
    if (running.current) return undefined;
    running.current = true;
    lastArgs.current = args;
    setBusy(true);
    setError(null);
    setConflict(false);
    try {
      return await actionRef.current(...args);
    } catch (cause) {
      setError(getErrorMessage(cause));
      setConflict(isConflict(cause));
      lastArgs.current = isConflict(cause) ? null : args;
      return undefined;
    } finally {
      running.current = false;
      setBusy(false);
    }
  }, []);

  const retry = useCallback(() => {
    if (lastArgs.current) void run(...lastArgs.current);
  }, [run]);

  const clear = useCallback(() => {
    setError(null);
    setConflict(false);
    lastArgs.current = null;
  }, []);

  return { run, retry, clear, busy, error, conflict };
}
