import { useState, useEffect, useCallback, useRef } from 'react';

import { useJobStatus } from '@/features/jobs/api';
import { VaultAPI } from '@/lib/api';
import type { FileIndexingOptionsDto } from '@/lib/bindings';
import type { ApiResult, BatchJobItem, BatchJobStatus } from '@/types';
import { listAllBatchJobs } from '@/utils/batchHistory';

export interface IndexingOperation {
  id: string;
  totalFiles: number;
  processedFiles: number;
  successfulFiles: number;
  failedFiles: number;
  currentFile?: string;
  status: 'pending' | 'processing' | 'completed' | 'cancelled' | 'error';
  error?: string;
  items?: BatchJobItem[];
}

/** How soon a status read that failed is tried again. */
const STATUS_RETRY_MS = 1000;

/** The statuses the backend stops writing to an item. */
const TERMINAL_ITEM_STATUSES = ['completed', 'failed', 'cancelled'];

/**
 * True while the backend has not yet recorded an outcome for every file.
 *
 * Cancelling a job does not decide the file that was already in flight: the
 * worker still finishes or abandons it, and only the item status says which.
 */
export const hasUnsettledItems = (operation: IndexingOperation): boolean =>
  operation.items === undefined ||
  operation.items.some((item) => !TERMINAL_ITEM_STATUSES.includes(item.status.toLowerCase()));

interface UseIndexingReturn {
  operations: Map<string, IndexingOperation>;
  isIndexing: boolean;
  historyError?: string;
  startBatchImport: (filePaths: string[], spaceId?: string, indexing?: FileIndexingOptionsDto) => Promise<ApiResult<string>>;
  cancelBatchImport: (id: string) => Promise<ApiResult<number>>;
  getOperation: (id: string) => IndexingOperation | undefined;
  clearOperation: (id: string) => void;
}

export function useIndexing(): UseIndexingReturn {
  const [historyError, setHistoryError] = useState<string>();
  const [operations, setOperations] = useState<Map<string, IndexingOperation>>(new Map());
  const operationsRef = useRef<Map<string, IndexingOperation>>(operations);

  const updateOperations = useCallback(
    (updater: (prev: Map<string, IndexingOperation>) => Map<string, IndexingOperation>) => {
      setOperations((prev) => {
        const next = updater(prev);
        operationsRef.current = next;
        return next;
      });
    },
    []
  );

  const mounted = useRef(true);
  const inFlight = useRef(new Set<string>());
  const again = useRef(new Set<string>());
  const retryTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  /**
   * Reads one import's status and files. One request per import at a time; a
   * change reported meanwhile reads it again after. A failed read is tried
   * again shortly, since no further report may come.
   */
  const refresh = useCallback(async (id: string): Promise<void> => {
    if (inFlight.current.has(id)) {
      again.current.add(id);
      return;
    }
    inFlight.current.add(id);
    const retryLater = (message: string) => {
      updateOperations((prev) => {
        const next = new Map(prev);
        const existing = next.get(id);
        if (!existing) return next;
        next.set(id, { ...existing, error: `Could not refresh import status: ${message}. Checking again…` });
        return next;
      });
      clearTimeout(retryTimers.current.get(id));
      retryTimers.current.set(id, setTimeout(() => {
        retryTimers.current.delete(id);
        if (mounted.current && operationsRef.current.has(id)) void refresh(id);
      }, STATUS_RETRY_MS));
    };
    try {
      const result = await VaultAPI.getBatchJobStatus(id);
      if (!mounted.current) return;
      if (!result.ok) {
        retryLater(result.error);
        return;
      }
      updateOperations((prev) => {
        const next = new Map(prev);
        const existing = next.get(id);
        const completedItems = result.data.completedItems ?? result.data.completed_items ?? 0;
        const failedItems = result.data.failedItems ?? result.data.failed_items ?? 0;
        const currentItem = result.data.items?.find((item) =>
          ['running', 'processing'].includes(item.status.toLowerCase())
        );
        next.set(id, {
          id,
          totalFiles: result.data.totalItems ?? result.data.total_items ?? 0,
          processedFiles: completedItems + failedItems,
          successfulFiles: completedItems,
          failedFiles: failedItems,
          status: mapBatchStatus(result.data),
          error: undefined,
          items: result.data.items || existing?.items,
          currentFile: currentItem?.target || currentItem?.url,
        });
        return next;
      });
    } catch (error) {
      if (mounted.current) retryLater(error instanceof Error ? error.message : 'Failed to fetch batch status');
    } finally {
      inFlight.current.delete(id);
      if (again.current.delete(id) && mounted.current) void refresh(id);
    }
  }, [updateOperations]);

  // Each file an import settles is reported as a job status; read the import
  // again when one of ours moves.
  useJobStatus((job) => {
    if (operationsRef.current.has(job.id)) void refresh(job.id);
  });

  // Reattach to imports after navigation or an app restart.
  useEffect(() => {
    mounted.current = true;
    const timers = retryTimers.current;
    void listAllBatchJobs().then((jobs) => {
      if (!mounted.current) return;
      setHistoryError(undefined);
      const reattached: string[] = [];
      updateOperations((prev) => {
        const next = new Map(prev);
        for (const job of jobs) {
          if (job.jobType !== 'file_import' || (!['pending', 'running'].includes(job.status) && job.failedItems === 0)) continue;
          const id = job.jobId || job.id;
          if (next.has(id)) continue;
          reattached.push(id);
          next.set(id, {
            id, totalFiles: job.totalItems,
            successfulFiles: job.completedItems, failedFiles: job.failedItems,
            processedFiles: job.completedItems + job.failedItems, status: 'processing',
          });
        }
        return next;
      });
      for (const id of reattached) void refresh(id);
    }).catch((error: unknown) => {
      if (mounted.current) setHistoryError(error instanceof Error ? error.message : 'Import status is unavailable');
    });
    return () => {
      mounted.current = false;
      for (const timer of timers.values()) clearTimeout(timer);
      timers.clear();
    };
  }, [refresh, updateOperations]);

  const startBatchImport = useCallback(async (filePaths: string[], spaceId?: string, indexing?: FileIndexingOptionsDto): Promise<ApiResult<string>> => {
    const result = await VaultAPI.batchFileImport(filePaths, spaceId, indexing);

    if (result.ok && result.data) {
      updateOperations((prev) => {
        const next = new Map(prev);
        next.set(result.data, {
          id: result.data,
          totalFiles: filePaths.length,
          processedFiles: 0,
          successfulFiles: 0,
          failedFiles: 0,
          status: 'pending',
        });
        return next;
      });
      void refresh(result.data);
    }

    return result;
  }, [refresh, updateOperations]);

  const cancelBatchImport = useCallback(async (id: string): Promise<ApiResult<number>> => {
    const result = await VaultAPI.cancelBatchJob(id);
    if (result.ok) {
      // The job is cancelled, but each file's outcome stays whatever the
      // backend last reported: the file in flight may still finish indexing,
      // and the job's final report reads it again once the worker stops.
      updateOperations((prev) => {
        const next = new Map(prev);
        const operation = next.get(id);
        if (operation) {
          next.set(id, { ...operation, status: 'cancelled', error: undefined });
        }
        return next;
      });
    }
    return result;
  }, [updateOperations]);

  const getOperation = useCallback(
    (id: string): IndexingOperation | undefined => operations.get(id),
    [operations]
  );

  const clearOperation = useCallback((id: string) => {
    updateOperations((prev) => {
      const next = new Map(prev);
      next.delete(id);
      return next;
    });
  }, [updateOperations]);

  const isIndexing = Array.from(operations.values()).some(
    (op) => op.status === 'pending' || op.status === 'processing'
  );

  return {
    operations,
    isIndexing,
    historyError,
    startBatchImport,
    cancelBatchImport,
    getOperation,
    clearOperation,
  };
}

function mapBatchStatus(status: BatchJobStatus): IndexingOperation['status'] {
  const normalized = status.status.toLowerCase();
  switch (normalized) {
    case 'pending':
      return 'pending';
    case 'running':
    case 'processing':
    case 'in_progress':
      return 'processing';
    case 'completed':
    case 'success':
      return (status.failedItems ?? status.failed_items ?? 0) > 0 ? 'error' : 'completed';
    case 'failed':
    case 'error':
      return 'error';
    case 'cancelled':
    case 'canceled':
      return 'cancelled';
    default:
      return 'processing';
  }
}
