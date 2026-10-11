import { useEffect, useRef } from 'react';

import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { JobDto } from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import { unwrapApiResult } from '@/types/api/result';

export type { JobDto } from '@/lib/bindings';

/** Carries a `JobDto` after every transition and progress write of any job. */
export const JOB_STATUS_EVENT = 'jobs://status';

export const jobKeys = {
  all: ['jobs'] as const,
  list: (kinds: readonly string[]) => ['jobs', 'list', [...kinds].sort().join(',')] as const,
};

/** Whether a job has stopped for good: it will not move again on its own. */
export function isJobFinished(job: Pick<JobDto, 'status'>): boolean {
  return job.status !== 'pending' && job.status !== 'running';
}

type JobSubscriber = (job: JobDto) => void;

const subscribers = new Set<JobSubscriber>();
let listening: Promise<UnlistenFn> | null = null;

/**
 * Calls `subscriber` with each job status the backend publishes. Every
 * subscriber shares one window listener, attached while any is subscribed.
 */
export function onJobStatus(subscriber: JobSubscriber): () => void {
  subscribers.add(subscriber);
  listening ??= listen<JobDto>(JOB_STATUS_EVENT, (event) => {
    for (const notify of [...subscribers]) notify(event.payload);
  }).catch(() => () => undefined);
  return () => {
    subscribers.delete(subscriber);
    if (subscribers.size > 0 || !listening) return;
    const attached = listening;
    listening = null;
    void attached.then((unlisten) => unlisten());
  };
}

/** Runs `onJob` for each published job status while the component is mounted. */
export function useJobStatus(onJob: JobSubscriber): void {
  const latest = useRef(onJob);
  latest.current = onJob;
  useEffect(() => onJobStatus((job) => latest.current(job)), []);
}

export const jobsQueryOptions = (kinds: readonly string[]) =>
  queryOptions({
    queryKey: jobKeys.list(kinds),
    queryFn: async () => unwrapApiResult(await apiCall<JobDto[]>('list_jobs', { kinds: [...kinds] })),
    // Kept current by the status event, not by refetching.
    staleTime: Infinity,
  });

/** Replaces `job` in `jobs` by id, or appends it. */
export function upsertJob(jobs: readonly JobDto[], job: JobDto): JobDto[] {
  const at = jobs.findIndex((existing) => existing.id === job.id);
  if (at < 0) return [...jobs, job];
  const next = [...jobs];
  next[at] = job;
  return next;
}

/**
 * The jobs of `kinds` that were pending or running when first read, then every
 * status published for those kinds, finished ones included.
 */
export function useJobs(kinds: readonly string[]) {
  const client = useQueryClient();
  const key = jobKeys.list(kinds);
  const wanted = useRef(new Set(kinds));
  wanted.current = new Set(kinds);
  useJobStatus((job) => {
    if (!wanted.current.has(job.kind)) return;
    client.setQueryData<JobDto[]>(key, (jobs) => upsertJob(jobs ?? [], job));
  });
  return useQuery(jobsQueryOptions(kinds));
}
