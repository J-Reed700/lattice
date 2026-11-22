import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { IndexingOperation } from '@/features/files/hooks/useIndexing';
import { useBatchFileImportStore } from '@/features/files/stores/batchFileImportStore';
import { JOB_STATUS_EVENT } from '@/features/jobs/api';
import type { ApiClient } from '@/lib/api';
import { toast as globalToast } from '@/stores/toastStore';

interface ToastLike {
  error: (title: string, options?: { message?: string }) => unknown;
}

type AddDocumentsFn = (collectionId: string, documentIds: string[]) => Promise<unknown>;

interface PendingCollectionAssignment {
  jobId: string;
  collectionId: string;
  api: ApiClient;
  addDocuments?: AddDocumentsFn;
  claimed: boolean;
}

const STATUS_RETRY_MS = 1000;
const MAX_STATUS_RETRIES = 10;

const pendingAssignments = new Map<string, PendingCollectionAssignment>();
const handledCompletionJobs = new Set<string>();
const inFlightChecks = new Set<string>();
const recheckQueued = new Set<string>();
const retryTimers = new Map<string, ReturnType<typeof setTimeout>>();
const retryCounts = new Map<string, number>();

let activeListenerPromise: Promise<UnlistenFn> | null = null;
let resolvedUnlisten: UnlistenFn | null = null;
let listenerApi: ApiClient | null = null;

function teardownListener(): void {
  const attached = activeListenerPromise;
  const unlisten = resolvedUnlisten;
  activeListenerPromise = null;
  resolvedUnlisten = null;
  listenerApi = null;
  if (unlisten) {
    unlisten();
  } else if (attached) {
    void attached.then((fn) => fn());
  }
}

function removePendingJob(jobId: string): void {
  pendingAssignments.delete(jobId);
  recheckQueued.delete(jobId);
  retryCounts.delete(jobId);
  const timer = retryTimers.get(jobId);
  if (timer !== undefined) {
    clearTimeout(timer);
    retryTimers.delete(jobId);
  }
  if (pendingAssignments.size === 0) {
    teardownListener();
  }
}

function scheduleRetry(jobId: string): void {
  const entry = pendingAssignments.get(jobId);
  if (!entry || entry.claimed) return;
  const attempts = (retryCounts.get(jobId) ?? 0) + 1;
  if (attempts > MAX_STATUS_RETRIES) {
    removePendingJob(jobId);
    return;
  }
  retryCounts.set(jobId, attempts);
  const existingTimer = retryTimers.get(jobId);
  if (existingTimer !== undefined) {
    clearTimeout(existingTimer);
  }
  retryTimers.set(
    jobId,
    setTimeout(() => {
      retryTimers.delete(jobId);
      void checkPendingJobStatus(jobId);
    }, STATUS_RETRY_MS),
  );
}

async function fallbackAddDocuments(
  api: ApiClient,
  collectionId: string,
  documentIds: string[],
): Promise<void> {
  const result = await api.addDocumentsToCustomCollection(collectionId, documentIds);
  if (!result.ok) {
    throw new Error(result.error);
  }
}

function executeCollectionAssignment(
  entry: PendingCollectionAssignment,
  documentIds: string[],
  options?: {
    addDocuments?: AddDocumentsFn;
    toast?: ToastLike;
  },
): void {
  if (documentIds.length === 0) return;

  const add = options?.addDocuments ?? entry.addDocuments;
  const operation = add
    ? add(entry.collectionId, documentIds)
    : fallbackAddDocuments(entry.api, entry.collectionId, documentIds);

  void Promise.resolve(operation).catch((error: unknown) => {
    const notify = options?.toast ?? globalToast;
    notify.error('Imported documents could not be added to the collection', {
      message: error instanceof Error ? error.message : String(error),
    });
  });
}

async function checkPendingJobStatus(jobId: string): Promise<void> {
  const entry = pendingAssignments.get(jobId);
  if (!entry || entry.claimed) return;
  if (inFlightChecks.has(jobId)) {
    recheckQueued.add(jobId);
    return;
  }
  const api = entry.api ?? listenerApi;
  if (!api || typeof api.getBatchJobStatus !== 'function') return;

  inFlightChecks.add(jobId);
  try {
    const result = await api.getBatchJobStatus(jobId);
    const current = pendingAssignments.get(jobId);
    if (!current || current.claimed) return;
    if (!result.ok) {
      scheduleRetry(jobId);
      return;
    }

    retryCounts.delete(jobId);
    const normalized = result.data.status.toLowerCase();
    if (normalized === 'cancelled' || normalized === 'canceled') {
      removePendingJob(jobId);
      return;
    }

    if (!['completed', 'success', 'failed', 'error', 'interrupted'].includes(normalized)) {
      return;
    }

    current.claimed = true;
    removePendingJob(jobId);

    const documentIds =
      result.data.items
        ?.filter((item) => item.status.toLowerCase() === 'completed' && item.documentId)
        .map((item) => item.documentId as string) ?? [];

    executeCollectionAssignment(current, documentIds);
  } catch (error) {
    console.error('[batchFileImportWatcher] Failed to check batch job status:', error);
    scheduleRetry(jobId);
  } finally {
    inFlightChecks.delete(jobId);
    if (recheckQueued.delete(jobId) && pendingAssignments.has(jobId)) {
      void checkPendingJobStatus(jobId);
    }
  }
}

function ensureJobStatusListener(api: ApiClient): void {
  if (activeListenerPromise || typeof api.getBatchJobStatus !== 'function') {
    return;
  }
  listenerApi = api;
  const promise = listen<{ id?: string }>(JOB_STATUS_EVENT, (event) => {
    const jobId = event.payload?.id;
    if (!jobId || !pendingAssignments.has(jobId)) return;
    retryCounts.delete(jobId);
    void checkPendingJobStatus(jobId);
  })
    .then((fn) => {
      if (activeListenerPromise !== promise || pendingAssignments.size === 0) {
        fn();
        return () => undefined;
      }
      resolvedUnlisten = fn;
      for (const jobId of [...pendingAssignments.keys()]) {
        void checkPendingJobStatus(jobId);
      }
      return fn;
    })
    .catch((error: unknown) => {
      if (activeListenerPromise === promise) {
        activeListenerPromise = null;
        resolvedUnlisten = null;
        listenerApi = null;
      }
      console.error('[batchFileImportWatcher] Failed to listen for job status:', error);
      return () => undefined;
    });

  activeListenerPromise = promise;
}

/** Persists `collectionId` for `jobId` and watches backend job completion across unmounts. */
export function registerJobCollectionWatcher(params: {
  jobId: string;
  collectionId: string;
  api: ApiClient;
  addDocuments?: AddDocumentsFn;
}): void {
  const { jobId, collectionId, api, addDocuments } = params;
  if (!jobId || !collectionId) return;
  pendingAssignments.set(jobId, {
    jobId,
    collectionId,
    api,
    addDocuments,
    claimed: false,
  });
  ensureJobStatusListener(api);
  if (resolvedUnlisten) {
    void checkPendingJobStatus(jobId);
  }
}

/**
 * Assigns imported documents from a settled operation to the persisted
 * collection for `operation.id` at most once.
 */
export function assignCollectionDocumentsFromOperation(
  operation: Pick<IndexingOperation, 'id' | 'status' | 'items'>,
  options?: {
    addDocuments?: AddDocumentsFn;
    toast?: ToastLike;
  },
): void {
  if (!['completed', 'error'].includes(operation.status)) {
    if (operation.status === 'cancelled') {
      removePendingJob(operation.id);
    }
    return;
  }

  const entry = pendingAssignments.get(operation.id);
  if (!entry || entry.claimed) return;

  entry.claimed = true;
  removePendingJob(operation.id);

  const documentIds =
    operation.items
      ?.filter((item) => item.status.toLowerCase() === 'completed' && item.documentId)
      .map((item) => item.documentId as string) ?? [];

  executeCollectionAssignment(entry, documentIds, options);
}

/** Marks a historical job as already settled so revisiting `/ingest -> Files` never re-toasts. */
export function markFileImportCompletionHandled(jobId: string): void {
  if (jobId) {
    handledCompletionJobs.add(jobId);
  }
}

/** Clears any prior notification mark when a user explicitly starts a fresh job. */
export function clearFileImportCompletionNotification(jobId: string): void {
  if (jobId) {
    handledCompletionJobs.delete(jobId);
  }
}

/**
 * Claims the one-shot completion notification for `jobId`.
 * Returns false if the job was restored from history or already notified.
 */
export function claimFileImportCompletionNotification(jobId: string): boolean {
  if (!jobId || handledCompletionJobs.has(jobId)) {
    return false;
  }
  handledCompletionJobs.add(jobId);
  return true;
}

/** Resets watcher and store state for test isolation. */
export function resetBatchFileImportWorkflow(): void {
  pendingAssignments.clear();
  handledCompletionJobs.clear();
  inFlightChecks.clear();
  recheckQueued.clear();
  retryCounts.clear();
  for (const timer of retryTimers.values()) {
    clearTimeout(timer);
  }
  retryTimers.clear();
  teardownListener();
  useBatchFileImportStore.getState().reset();
}
