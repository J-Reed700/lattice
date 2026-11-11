import { useCallback, useEffect, useState } from 'react';

import { isJobFinished, useJobStatus } from '@/features/jobs/api';
import type { CourseDrillsDto } from '@/lib/bindings';

import { drillsApi } from './api';
import { DRILL_BUILD } from './drillJobs';

/** A course's drills view in a language, read again when a build of the course finishes. */
export function useCourseDrills(
  programId: string,
  language: string | null,
): { view: CourseDrillsDto | null; error: string | null; reload: () => void } {
  const [view, setView] = useState<CourseDrillsDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [round, setRound] = useState(0);

  useEffect(() => {
    let current = true;
    setError(null);
    drillsApi.courseDrills(programId, language).then(
      (found) => {
        if (current) setView(found);
      },
      (failure: unknown) => {
        if (current) setError(failure instanceof Error ? failure.message : String(failure));
      },
    );
    return () => {
      current = false;
    };
  }, [programId, language, round]);

  const reload = useCallback(() => setRound((previous) => previous + 1), []);

  useJobStatus((job) => {
    if (job.kind === DRILL_BUILD && job.subjectId?.startsWith(`drill:${programId}:`) && isJobFinished(job)) reload();
  });

  return { view, error, reload };
}
