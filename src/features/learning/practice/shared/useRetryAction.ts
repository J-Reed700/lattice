import { useCallback, useState } from 'react';

export interface RetryAction {
  label: string;
  run: () => Promise<void> | void;
}

export interface UseRetryActionResult {
  retryAction: RetryAction | null;
  setRetryAction: (action: RetryAction | null) => void;
  clearRetry: () => void;
  runRetry: () => Promise<void>;
}

/**
 * Shared retry hook for transient operation failures across practice and sources panels.
 */
export function useRetryAction(): UseRetryActionResult {
  const [retryAction, setRetryAction] = useState<RetryAction | null>(null);

  const clearRetry = useCallback(() => {
    setRetryAction(null);
  }, []);

  const runRetry = useCallback(async () => {
    if (!retryAction) return;
    await retryAction.run();
  }, [retryAction]);

  return {
    retryAction,
    setRetryAction,
    clearRetry,
    runRetry,
  };
}
