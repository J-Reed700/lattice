import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query';

import { isJobFinished, newerJob, useJobStatus, type JobDto } from '@/features/jobs/api';
import type {
  JournalSynthesisDto,
  StartJournalSynthesisRequestDto,
  SynthesisActivityDto,
} from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import { unwrapApiResult } from '@/types/api/result';

export type { JournalSynthesisDto, SynthesisDestinationDto } from '@/lib/bindings';

/** The job kind of a journal synthesis; its activity is a `SynthesisActivityDto`. */
export const JOURNAL_SYNTHESIS_JOB = 'journal.synthesis';

export const synthesisKeys = {
  list: ['journal', 'syntheses'] as const,
};

/** Starts a synthesis as a job. Its destination is fixed now, whatever happens before it finishes. */
export const startSynthesis = async (request: StartJournalSynthesisRequestDto) =>
  unwrapApiResult(await apiCall('synthesize_journal_entries', { request }));

/** The result a finished synthesis staged, for the renderer to save. */
export const synthesisResult = async (jobId: string) =>
  unwrapApiResult(await apiCall('get_journal_synthesis_result', { jobId }));

/** Records that a finished synthesis was saved; false when it already had been. */
export const markSynthesisApplied = async (jobId: string) =>
  unwrapApiResult(await apiCall('mark_journal_synthesis_applied', { jobId }));

/** Removes a synthesis that ended without a result. */
export const dismissSynthesis = async (jobId: string) =>
  unwrapApiResult(await apiCall('dismiss_journal_synthesis', { jobId }));

/** Runs a failed or interrupted synthesis again, keeping the parts it wrote. */
export const retrySynthesis = async (jobId: string) =>
  unwrapApiResult(await apiCall('retry_journal_synthesis', { jobId }));

export const synthesesQueryOptions = () =>
  queryOptions({
    queryKey: synthesisKeys.list,
    queryFn: async ({ client }) => {
      const snapshot = unwrapApiResult(await apiCall('list_journal_syntheses'));
      return mergeSyntheses(client.getQueryData<JournalSynthesisDto[]>(synthesisKeys.list) ?? [], snapshot);
    },
    // Kept current by the job status event, not by refetching.
    staleTime: Infinity,
  });

/** Keeps `existing.job` only when its revision is strictly higher than `synthesis.job`. */
function keepNewerJob(
  synthesis: JournalSynthesisDto,
  existing: JournalSynthesisDto | undefined,
): JournalSynthesisDto {
  return existing && newerJob(existing.job, synthesis.job) !== synthesis.job
    ? withJob(synthesis, existing.job)
    : synthesis;
}

/** The snapshot's syntheses and order, each keeping a newer job the cache already holds. */
export function mergeSyntheses(
  cached: readonly JournalSynthesisDto[],
  snapshot: readonly JournalSynthesisDto[],
): JournalSynthesisDto[] {
  return snapshot.map((item) => keepNewerJob(item, cached.find((candidate) => candidate.job.id === item.job.id)));
}

/** Whether a synthesis is still being written. */
export function isSynthesisLive(synthesis: JournalSynthesisDto): boolean {
  return !isJobFinished(synthesis.job);
}

/** The synthesis's own record, updated from a job status the backend published. */
export function withJob(synthesis: JournalSynthesisDto, job: JobDto): JournalSynthesisDto {
  return { ...synthesis, job, activity: (job.activity as SynthesisActivityDto | null) ?? synthesis.activity };
}

/**
 * Every synthesis not yet saved or dismissed — running, finished while the
 * app was closed, or failed — kept current by `jobs://status`. A synthesis
 * this window has not seen yet is read again from the backend.
 */
export function useJournalSyntheses() {
  const client = useQueryClient();
  useJobStatus((job) => {
    if (job.kind !== JOURNAL_SYNTHESIS_JOB) return;
    const syntheses = client.getQueryData<JournalSynthesisDto[]>(synthesisKeys.list);
    if (!syntheses?.some((synthesis) => synthesis.job.id === job.id)) {
      void client.invalidateQueries({ queryKey: synthesisKeys.list });
      return;
    }
    client.setQueryData<JournalSynthesisDto[]>(synthesisKeys.list, syntheses.map((synthesis) => {
      if (synthesis.job.id !== job.id) return synthesis;
      const chosen = newerJob(synthesis.job, job);
      return chosen === synthesis.job ? synthesis : withJob(synthesis, chosen);
    }));
  });
  return useQuery(synthesesQueryOptions());
}

/** Puts a synthesis the backend just returned at the front of the list, replacing any older record of it. */
export function rememberSynthesis(
  syntheses: readonly JournalSynthesisDto[] | undefined,
  synthesis: JournalSynthesisDto,
): JournalSynthesisDto[] {
  const existing = syntheses?.find((item) => item.job.id === synthesis.job.id);
  return [
    keepNewerJob(synthesis, existing),
    ...(syntheses ?? []).filter((item) => item.job.id !== synthesis.job.id && item.job.id !== synthesis.job.retryOfJobId),
  ];
}
