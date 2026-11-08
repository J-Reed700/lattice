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
 * Minimal Stage 1 non-panicking stub for useRetryAction.
 */
export function useRetryAction(): UseRetryActionResult {
  return {
    retryAction: null,
    setRetryAction: () => {},
    clearRetry: () => {},
    runRetry: async () => {},
  };
}
