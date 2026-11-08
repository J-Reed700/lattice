export type AutosaveStatus = 'idle' | 'saving' | 'saved' | 'error' | 'conflict';

export interface UseAutosaveOptions<T> {
  value: T;
  onSave: (value: T) => Promise<unknown>;
  delayMs?: number;
  isEqual?: (a: T, b: T) => boolean;
  enabled?: boolean;
}

export interface UseAutosaveResult<T> {
  status: AutosaveStatus;
  error: string | null;
  conflict: boolean;
  clearConflict: () => void;
  flush: () => Promise<boolean>;
  retry: () => Promise<void>;
  reset: () => void;
}

/**
 * Checks if an error represents an optimistic locking / revision conflict.
 * Uses structured error details (`details?.code === 'CONFLICT'`) rather than regex heuristics.
 */
export function isConflictError(_error: unknown): boolean {
  return false;
}

/**
 * Minimal Stage 1 non-panicking stub for useAutosave.
 * Returns initial idle state so state transition tests fail cleanly.
 */
export function useAutosave<T>(_options: UseAutosaveOptions<T>): UseAutosaveResult<T> {
  return {
    status: 'idle',
    error: null,
    conflict: false,
    clearConflict: () => {},
    flush: async () => false,
    retry: async () => {},
    reset: () => {},
  };
}
