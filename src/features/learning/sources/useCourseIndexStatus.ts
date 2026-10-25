import { useCallback, useEffect } from 'react';

import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { useJobStatus, type JobDto } from '@/features/jobs/api';
import VaultAPI, { type ApiClient } from '@/lib/api';
import type { CourseIndexStatusDto } from '@/lib/bindings';
import { unwrapApiResult } from '@/types/api/result';

import { describeIndexWait } from './courseIndexProgress';

/** The job kind of a course index build; its subject is the course. */
export const COURSE_INDEX_JOB = 'learning.course_index';

export const courseIndexKeys = {
  status: (programId: string) => ['learning', 'course-index', programId] as const,
};

const statusQuery = (programId: string, api: ApiClient = VaultAPI) =>
  queryOptions<CourseIndexStatusDto>({
    queryKey: courseIndexKeys.status(programId),
    queryFn: async () => unwrapApiResult(await api.courseIndexStatus(programId)),
  });

/** The course's index status as last read; `null` until it has been. */
export function useCourseIndexStatus(programId: string | null | undefined): CourseIndexStatusDto | null {
  const api = useApi();
  const { data } = useQuery({ ...statusQuery(programId ?? '', api), enabled: Boolean(programId) });
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
  const api = useApi();
  useEffect(() => {
    if (!programId) return undefined;
    void api.courseIndexOpen(programId);
    return () => {
      void api.courseIndexClose(programId);
    };
  }, [api, programId]);
}

/**
 * Why lesson preparation (prepare, retry, resume) and resumed verification
 * wait for the course's index, in words that follow it live: "Waits for the
 * course index · about 11 min left". Only while it is indexing, since a
 * lesson's run waits for the embedding anyway. Source search and attaching
 * evidence never wait: each brings the index's words up to date itself and
 * searches what is embedded by meaning, the rest by words. Searching by
 * words alone (no model, or an index that cannot be read) holds nothing
 * back. Returns the reason to show beside the held control, or `undefined`.
 */
export function useEvidenceWaitsForIndex(programId: string | null | undefined): string | undefined {
  return describeIndexWait(useCourseIndexStatus(programId));
}
