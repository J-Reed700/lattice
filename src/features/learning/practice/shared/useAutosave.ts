import { useCallback, useEffect, useRef, useState } from 'react';

import { isConflict } from '@/lib/api';
import { getErrorMessage } from '@/lib/errorUtils';

/** How long typing must pause before a change is saved. */
export const AUTOSAVE_DELAY_MS = 650;

export type AutosaveState = 'idle' | 'saving' | 'saved' | 'conflict' | 'error';

/** What the save indicator says: saving, a change still waiting or a failed save, or saved. */
export const saveStatusOf = (state: AutosaveState, dirty: boolean): 'saving' | 'unsaved' | 'saved' =>
  state === 'saving' ? 'saving' : dirty || state === 'error' || state === 'conflict' ? 'unsaved' : 'saved';

export interface UseAutosaveOptions<T> {
  value: T;
  /** The revision the next save is made against; each save moves on to the one it returns. */
  revision: number;
  save: (value: T, expectedRevision: number) => Promise<{ revision: number }>;
  delayMs?: number;
  /** While false nothing is saved. Turning it on takes the current value as already saved. */
  enabled?: boolean;
}

export interface UseAutosaveResult {
  state: AutosaveState;
  error: string | null;
  /** Saves now if the value differs from the last saved one, and resolves once the attempt ends. */
  flush: () => Promise<void>;
}

/**
 * Saves `value` after a pause in changes, one save at a time, against a revision.
 * A failed save is not repeated until the value changes, `flush` is called, or, after
 * a conflict, a newer revision arrives.
 */
export function useAutosave<T>({
  value,
  revision,
  save,
  delayMs = AUTOSAVE_DELAY_MS,
  enabled = true,
}: UseAutosaveOptions<T>): UseAutosaveResult {
  const [state, setState] = useState<AutosaveState>('idle');
  const [error, setError] = useState<string | null>(null);

  const latest = useRef({ value, save });
  latest.current = { value, save };
  const revisionRef = useRef(revision);
  const savedRef = useRef(value);
  const stateRef = useRef<AutosaveState>('idle');
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const chainRef = useRef<Promise<void>>(Promise.resolve());
  const mountedRef = useRef(true);

  const report = useCallback((next: AutosaveState, message: string | null) => {
    stateRef.current = next;
    if (!mountedRef.current) return;
    setState(next);
    setError(message);
  }, []);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  const saveIfChanged = useCallback(async () => {
    const current = latest.current.value;
    if (Object.is(current, savedRef.current)) return;
    report('saving', null);
    try {
      const saved = await latest.current.save(current, revisionRef.current);
      savedRef.current = current;
      revisionRef.current = saved.revision;
      report('saved', null);
    } catch (cause) {
      report(isConflict(cause) ? 'conflict' : 'error', getErrorMessage(cause, 'Your work could not be saved.'));
    }
  }, [report]);

  const flush = useCallback(() => {
    clearTimer();
    chainRef.current = chainRef.current.then(saveIfChanged);
    return chainRef.current;
  }, [clearTimer, saveIfChanged]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      clearTimer();
    };
  }, [clearTimer]);

  useEffect(() => {
    if (!enabled) return;
    savedRef.current = latest.current.value;
    report('idle', null);
  }, [enabled, report]);

  useEffect(() => {
    revisionRef.current = revision;
    if (enabled && stateRef.current === 'conflict') void flush();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision]);

  useEffect(() => {
    if (!enabled || Object.is(value, savedRef.current)) return;
    clearTimer();
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      void flush();
    }, delayMs);
    return clearTimer;
  }, [value, enabled, delayMs, clearTimer, flush]);

  return { state, error, flush };
}
