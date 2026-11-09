export type AutosaveState = 'idle' | 'saving' | 'saved' | 'conflict' | 'error';

export interface UseAutosaveOptions<T> {
  value: T;
  revision: number;
  save: (value: T, expectedRevision: number) => Promise<{ revision: number }>;
  delayMs?: number;
  enabled?: boolean;
}

export interface UseAutosaveResult {
  state: AutosaveState;
  error: string | null;
  flush: () => Promise<void>;
}

export const AUTOSAVE_DELAY_MS = 650;

export function useAutosave<T>(_options: UseAutosaveOptions<T>): UseAutosaveResult {
  return { state: 'idle', error: null, flush: async () => {} };
}

export const isConflictError = (_error: unknown): boolean => false;
