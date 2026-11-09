import { useCallback, useRef, useState } from 'react';

import { getErrorMessage } from '@/lib/errorUtils';

/**
 * Runs an action that can fail, keeping its error text and the arguments of its
 * last run so Try again repeats it exactly. A run while one is going does nothing.
 */
export function useRetry<A extends unknown[], R>(action: (...args: A) => Promise<R>) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
    try {
      return await actionRef.current(...args);
    } catch (cause) {
      setError(getErrorMessage(cause));
      return undefined;
    } finally {
      running.current = false;
      setBusy(false);
    }
  }, []);

  const retry = useCallback(() => {
    if (lastArgs.current) void run(...lastArgs.current);
  }, [run]);

  return { run, retry, busy, error };
}
