import { useCallback, useEffect, useRef, useState } from 'react';

export type AutosaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error' | 'conflict';
export type AutosaveState = AutosaveStatus;

export interface UseAutosaveOptions<T> {
  value: T;
  saved?: T;
  onSave?: (value: T) => Promise<unknown>;
  save?: (value: T) => Promise<unknown>;
  delayMs?: number;
  isEqual?: (a: T, b: T) => boolean;
  equals?: (a: T, b: T) => boolean;
  enabled?: boolean;
}

export interface UseAutosaveResult {
  status: AutosaveStatus;
  state: AutosaveState;
  error: string | null;
  conflict: boolean;
  clearConflict: () => void;
  flush: () => Promise<boolean>;
  retry: () => Promise<void>;
  reset: () => void;
}

export type Autosave = UseAutosaveResult;

/**
 * Checks if an error represents an optimistic locking / revision conflict.
 * Uses structured error details (`details?.code === 'CONFLICT'`) rather than regex heuristics.
 */
export function isConflictError(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false;
  const candidate = error as { details?: { code?: string }; code?: string };
  return candidate.details?.code === 'CONFLICT' || candidate.code === 'CONFLICT';
}

export const isConflict = isConflictError;

const defaultIsEqual = <T>(a: T, b: T): boolean => a === b;

/**
 * Shared autosave hook with debounce, flush, retry, and structured conflict handling.
 */
export function useAutosave<T>({
  value,
  saved,
  onSave,
  save,
  delayMs = 500,
  isEqual,
  equals,
  enabled = true,
}: UseAutosaveOptions<T>): UseAutosaveResult {
  const [status, setStatus] = useState<AutosaveStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);

  const valueRef = useRef<T>(value);
  valueRef.current = value;

  const savedValueRef = useRef<T>(saved !== undefined ? saved : value);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef<Promise<boolean> | null>(null);
  const saveFn = onSave ?? save;
  const onSaveRef = useRef(saveFn);
  onSaveRef.current = saveFn;
  const equalFn = isEqual ?? equals ?? defaultIsEqual;
  const isEqualRef = useRef(equalFn);
  isEqualRef.current = equalFn;
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, []);

  const clearConflict = useCallback(() => {
    if (!mountedRef.current) return;
    setConflict(false);
    setError(null);
    setStatus('idle');
  }, []);

  const performSave = useCallback(async (valToSave: T): Promise<boolean> => {
    if (inFlightRef.current) return inFlightRef.current;

    const promise = (async () => {
      if (mountedRef.current) {
        setStatus('saving');
        setError(null);
      }
      try {
        await onSaveRef.current?.(valToSave);
        savedValueRef.current = valToSave;
        if (mountedRef.current) {
          setStatus('saved');
          setConflict(false);
          setError(null);
        }
        return true;
      } catch (err) {
        const conflictDetected = isConflictError(err);
        const errorMessage =
          err instanceof Error
            ? err.message
            : typeof (err as { message?: string })?.message === 'string'
              ? (err as { message: string }).message
              : 'Failed to save';

        if (mountedRef.current) {
          if (conflictDetected) {
            setStatus('conflict');
            setConflict(true);
            setError(errorMessage);
          } else {
            setStatus('error');
            setConflict(false);
            setError(errorMessage);
          }
        }
        return false;
      } finally {
        inFlightRef.current = null;
      }
    })();

    inFlightRef.current = promise;
    return promise;
  }, []);

  const flush = useCallback(async (): Promise<boolean> => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    const currentVal = valueRef.current;
    if (isEqualRef.current(currentVal, savedValueRef.current)) {
      return true;
    }
    return performSave(currentVal);
  }, [performSave]);

  const retry = useCallback(async (): Promise<void> => {
    await flush();
  }, [flush]);

  const reset = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    savedValueRef.current = valueRef.current;
    if (mountedRef.current) {
      setStatus('idle');
      setConflict(false);
      setError(null);
    }
  }, []);

  // Debounced autosave effect on value changes
  useEffect(() => {
    if (!enabled) return;
    if (isEqualRef.current(value, savedValueRef.current)) {
      return;
    }

    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }

    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      void performSave(valueRef.current);
    }, delayMs);

    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [value, delayMs, enabled, performSave]);

  return {
    status,
    state: status,
    error,
    conflict,
    clearConflict,
    flush,
    retry,
    reset,
  };
}
