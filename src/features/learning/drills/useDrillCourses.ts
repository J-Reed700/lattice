import { useCallback, useEffect, useState } from 'react';

import type { ReviewDrillCourseDto } from '@/lib/bindings';

import { drillsApi } from './api';

/** The courses a drill on a passage can draw on: those with at least one source. Read only while `enabled`. */
export function useDrillCourses(enabled: boolean) {
  const [courses, setCourses] = useState<ReviewDrillCourseDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [round, setRound] = useState(0);

  useEffect(() => {
    if (!enabled) return;
    let current = true;
    setError(null);
    drillsApi.reviewDrillCourses().then(
      (found) => {
        if (current) setCourses(found);
      },
      (failure: unknown) => {
        if (current) setError(failure instanceof Error ? failure.message : String(failure));
      },
    );
    return () => {
      current = false;
    };
  }, [enabled, round]);

  const reload = useCallback(() => setRound((previous) => previous + 1), []);
  return { courses, error, reload };
}
