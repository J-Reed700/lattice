import type { ApiResult } from '@/types';
import type { BatchJobStatus } from '@/types/api/batch';

/** Consecutive failed status reads before the send gives up on the job. */
export const MAX_CONSECUTIVE_POLL_FAILURES = 5;

export type AttachmentImportOutcome =
  /** The job finished: `added` may be 0 when every file failed. */
  | { kind: 'finished'; documentIds: string[]; added: number; failed: number }
  /** Still indexing at the deadline, with the ids the job has created so far. */
  | { kind: 'pending'; documentIds: string[] }
  /** The status could not be read, repeatedly. */
  | { kind: 'unreachable'; error: string };

interface PollOptions {
  getStatus: (jobId: string) => Promise<ApiResult<BatchJobStatus>>;
  intervalMs: number;
  timeoutMs: number;
  sleep?: (ms: number) => Promise<void>;
  now?: () => number;
}

const defaultSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function documentIdsOf(job: BatchJobStatus): string[] {
  return (job.items ?? [])
    .map((item) => item.documentId)
    .filter((id): id is string => Boolean(id));
}

/**
 * Poll a batch import until it finishes, the deadline passes, or its status
 * stays unreadable. Poll rather than subscribe: the batch slice emits no
 * per-job event.
 *
 * One failed read is not a verdict. The send used to stop at the first one and
 * go ahead with no document ids, so the answer was written without files whose
 * chips said they were attached.
 */
export async function pollAttachmentImport(
  jobId: string,
  { getStatus, intervalMs, timeoutMs, sleep = defaultSleep, now = Date.now }: PollOptions
): Promise<AttachmentImportOutcome> {
  const deadline = now() + timeoutMs;
  let lastSeenIds: string[] = [];
  let consecutiveFailures = 0;
  while (now() < deadline) {
    await sleep(intervalMs);
    const status = await getStatus(jobId);
    if (!status.ok) {
      consecutiveFailures += 1;
      if (consecutiveFailures >= MAX_CONSECUTIVE_POLL_FAILURES) {
        return { kind: 'unreachable', error: status.error };
      }
      continue;
    }
    consecutiveFailures = 0;
    const job = status.data;
    lastSeenIds = documentIdsOf(job);
    const terminal =
      job.status === 'completed' ||
      job.status === 'failed' ||
      job.status === 'cancelled' ||
      job.completedItems + job.failedItems >= job.totalItems;
    if (terminal) {
      return { kind: 'finished', documentIds: lastSeenIds, added: job.completedItems, failed: job.failedItems };
    }
  }
  return { kind: 'pending', documentIds: lastSeenIds };
}
