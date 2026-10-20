import { useCallback, useEffect } from 'react';

import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query';

import { useJobStatus, type JobDto } from '@/features/jobs/api';
import VaultAPI from '@/lib/api';
import type { CourseIndexStatusDto } from '@/lib/bindings';
import { unwrapApiResult } from '@/types/api/result';

/** The job kind of a course index build; its subject is the course. */
export const COURSE_INDEX_JOB = 'learning.course_index';

export const courseIndexKeys = {
  status: (programId: string) => ['learning', 'course-index', programId] as const,
};

const statusQuery = (programId: string) =>
  queryOptions<CourseIndexStatusDto>({
    queryKey: courseIndexKeys.status(programId),
    queryFn: async () => unwrapApiResult(await VaultAPI.courseIndexStatus(programId)),
  });

/** The course's index status as last read; `null` until it has been. */
export function useCourseIndexStatus(programId: string | null | undefined): CourseIndexStatusDto | null {
  const { data } = useQuery({ ...statusQuery(programId ?? ''), enabled: Boolean(programId) });
  return programId ? data ?? null : null;
}

/**
 * Reads the course's status again whenever one of its index builds moves:
 * queued, each progress write, finished. Mount once per open course (the
 * notice does); a status needs no timer, since a build that has not started
 * is still indexing.
 */
export function useCourseIndexStatusEvents(programId: string | null | undefined): void {
  const client = useQueryClient();
  const refresh = useCallback(
    (job: JobDto) => {
      if (job.kind === COURSE_INDEX_JOB && programId && job.subjectId === programId) {
        void client.invalidateQueries({ queryKey: courseIndexKeys.status(programId), exact: true });
      }
    },
    [client, programId]
  );
  useJobStatus(refresh);
}

/**
 * Tells the app the course's workspace is open, so its index goes first:
 * folder builds and other courses' builds hold while it embeds. Leaving the
 * workspace, or opening another course, gives the turn back.
 */
export function useCourseIndexOpen(programId: string | null | undefined): void {
  useEffect(() => {
    if (!programId) return undefined;
    void VaultAPI.courseIndexOpen(programId);
    return () => {
      void VaultAPI.courseIndexClose(programId);
    };
  }, [programId]);
}

/** Why a course's evidence actions wait, shown on each held control. */
export const EVIDENCE_WAITS_FOR_INDEX = 'Available once this course’s references finish indexing';

/**
 * Whether the course's evidence actions wait for its index: only while it
 * is indexing. Searching by words alone (no model, or an index that cannot
 * be read) holds nothing back. Returns the reason to show, or `undefined`.
 */
export function useEvidenceWaitsForIndex(programId: string | null | undefined): string | undefined {
  const status = useCourseIndexStatus(programId);
  return status?.state === 'indexing' ? EVIDENCE_WAITS_FOR_INDEX : undefined;
}
