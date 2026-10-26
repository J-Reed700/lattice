import { useCallback, useState } from 'react';

/**
 * What the learner typed into one part of a lesson (a check's answer, their
 * explanation from memory), kept on this device so leaving the lesson and
 * coming back does not erase their thinking. It is theirs, not a submission.
 */
export function useLessonDraft(key: string): [string, (next: string) => void] {
  const storageKey = `lattice.lesson-draft.${key}`;
  const [value, setValue] = useState(() => {
    try { return globalThis.localStorage?.getItem(storageKey) ?? ''; } catch { return ''; }
  });
  const update = useCallback((next: string) => {
    setValue(next);
    try {
      if (next) globalThis.localStorage?.setItem(storageKey, next);
      else globalThis.localStorage?.removeItem(storageKey);
    } catch { /* Storage full or unavailable: the text stays on screen. */ }
  }, [storageKey]);
  return [value, update];
}
