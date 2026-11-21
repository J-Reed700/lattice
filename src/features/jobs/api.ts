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

const MAX_FINISHED_JOBS = 100;

type JobSubscriber = (job: JobDto) => void;

const subscribers = new Set<JobSubscriber>();
let listening: Promise<UnlistenFn> | null = null;

/**
 * Calls `subscriber` with each job status the backend publishes. Every
 * subscriber shares one window listener, attached while any is subscribed.
 */
export function onJobStatus(subscriber: JobSubscriber): () => void {
  subscribers.add(subscriber);
  if (!listening) {
    const promise = listen<JobDto>(JOB_STATUS_EVENT, (event) => {
      for (const notify of [...subscribers]) notify(event.payload);
    }).catch((err: unknown) => {
      if (listening === promise) listening = null;
      console.error('[jobs] Failed to subscribe to job status events:', err);
      return () => undefined;
    });
    listening = promise;
  }
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
    queryFn: async () => unwrapApiResult(await apiCall('list_jobs', { kinds: [...kinds] })),
    // Kept current by the status event, not by refetching.
    staleTime: Infinity,
  });

function pickMonotonicJob(existing: JobDto, incoming: JobDto): JobDto {
  return incoming.revision >= existing.revision ? incoming : existing;
}

function capFinishedJobs(jobs: JobDto[]): JobDto[] {
  let finishedCount = 0;
  for (const job of jobs) {
    if (isJobFinished(job)) finishedCount += 1;
  }
  if (finishedCount <= MAX_FINISHED_JOBS) return jobs;

  const finishedWithIndex = jobs
    .map((job, index) => ({ job, index }))
    .filter(({ job }) => isJobFinished(job))
    .sort((a, b) => {
      const timeA = a.job.finishedAt ?? a.job.startedAt ?? a.job.createdAt;
      const timeB = b.job.finishedAt ?? b.job.startedAt ?? b.job.createdAt;
      if (timeB !== timeA) return timeB - timeA;
      return b.index - a.index;
    });
  const keepIndices = new Set(
    finishedWithIndex.slice(0, MAX_FINISHED_JOBS).map(({ index }) => index),
  );
  return jobs.filter((job, index) => !isJobFinished(job) || keepIndices.has(index));
}

/** Replaces `job` monotonically in `jobs` by id, or appends it, capping retained terminal jobs. */
export function upsertJob(jobs: readonly JobDto[], job: JobDto): JobDto[] {
  const at = jobs.findIndex((existing) => existing.id === job.id);
  if (at < 0) return capFinishedJobs([...jobs, job]);
  const existing = jobs[at]!;
  const chosen = pickMonotonicJob(existing, job);
  if (chosen === existing) return capFinishedJobs([...jobs]);
  const next = [...jobs];
  next[at] = chosen;
  return capFinishedJobs(next);
}

function mergeJobs(cached: readonly JobDto[], snapshot: readonly JobDto[]): JobDto[] {
  let merged: JobDto[] = capFinishedJobs([...snapshot]);
  for (const job of cached) {
    merged = upsertJob(merged, job);
  }
  return merged;
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
  const base = jobsQueryOptions(kinds);
  return useQuery({
    ...base,
    queryFn: async (ctx) => {
      const snapshot = await base.queryFn!(ctx);
      const cached = client.getQueryData<JobDto[]>(key) ?? [];
      return mergeJobs(cached, snapshot);
    },
  });
}
