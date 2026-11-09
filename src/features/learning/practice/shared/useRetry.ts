import { useCallback, useRef, useState } from 'react';

export {
  type RetryAction,
  useRetryAction,
  type UseRetryActionResult,
} from './useRetryAction';

export interface UseRetryResult<T> {
  run: () => Promise<T | undefined>;
  busy: boolean;
  error: string | null;
  retry: () => void;
  clear: () => void;
}

/**
 * Shared retry hook for async operations across practice surfaces.
 */
export function useRetry<T>(operation: () => Promise<T>): UseRetryResult<T> {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const opRef = useRef(operation);
  opRef.current = operation;

  const run = useCallback(async (): Promise<T | undefined> => {
    setBusy(true);
    setError(null);
    try {
      return await opRef.current();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return undefined;
    } finally {
      setBusy(false);
    }
  }, []);

  const retry = useCallback(() => {
    void run();
  }, [run]);

  const clear = useCallback(() => {
    setError(null);
  }, []);

  return { run, busy, error, retry, clear };
}
