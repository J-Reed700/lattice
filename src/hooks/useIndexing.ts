import { useState, useEffect, useCallback, useRef } from 'react';

import type { FileIndexingOptionsDto } from '@/lib/bindings';

import { VaultAPI } from '../lib/api';
import { listAllBatchJobs } from '../utils/batchHistory';

import type { ApiResult, BatchJobItem, BatchJobStatus } from '../types';

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

  // Poll all active operations without overlapping status requests.
  useEffect(() => {
    let mounted = true;
    let polling = false;

    // Reattach to imports after navigation or an app restart.
    void listAllBatchJobs().then((jobs) => {
      if (!mounted) return;
      setHistoryError(undefined);
      updateOperations((prev) => {
        const next = new Map(prev);
        for (const job of jobs) {
          if (job.jobType !== 'file_import' || (!['pending', 'running'].includes(job.status) && job.failedItems === 0)) continue;
          const id = job.jobId || job.id;
          if (next.has(id)) continue;
          next.set(id, {
            id, totalFiles: job.totalItems,
            successfulFiles: job.completedItems, failedFiles: job.failedItems,
            processedFiles: job.completedItems + job.failedItems, status: 'processing',
          });
        }
        return next;
      });
    }).catch((error: unknown) => {
      if (mounted) setHistoryError(error instanceof Error ? error.message : 'Import status is unavailable');
    });

    const poll = async () => {
      if (polling) return;
      polling = true;
      const activeOperations = Array.from(operationsRef.current.values()).filter(
        (op) =>
          op.status === 'pending' ||
          op.status === 'processing' ||
          (op.status === 'cancelled' && hasUnsettledItems(op))
      );

      for (const operation of activeOperations) {
        try {
          const result = await VaultAPI.getBatchJobStatus(operation.id);
          if (!mounted) continue;

          if (!result.ok) {
            updateOperations((prev) => {
              const next = new Map(prev);
              const existing = next.get(operation.id);
              if (!existing) return next;

              next.set(operation.id, {
                ...existing,
                error: `Could not refresh import status: ${result.error}. Checking again…`,
              });
              return next;
            });
            continue;
          }

          updateOperations((prev) => {
            const next = new Map(prev);
            const existing = next.get(operation.id);
            const status = mapBatchStatus(result.data);
            const completedItems = result.data.completedItems ?? result.data.completed_items ?? 0;
            const failedItems = result.data.failedItems ?? result.data.failed_items ?? 0;
            const totalItems = result.data.totalItems ?? result.data.total_items ?? 0;
            const processedFiles = completedItems + failedItems;
            const currentItem = result.data.items?.find((item) =>
              ['running', 'processing'].includes(item.status.toLowerCase())
            );

            next.set(operation.id, {
              id: operation.id,
              totalFiles: totalItems,
              processedFiles,
              successfulFiles: completedItems,
              failedFiles: failedItems,
              status,
              error: undefined,
              items: result.data.items || existing?.items,
              currentFile: currentItem?.target || currentItem?.url,
            });
            return next;
          });
        } catch (error) {
          if (!mounted) continue;

          const errorMessage =
            error instanceof Error ? error.message : 'Failed to fetch batch status';

          updateOperations((prev) => {
            const next = new Map(prev);
            const existing = next.get(operation.id);
            if (!existing) return next;

            next.set(operation.id, {
              ...existing,
              error: `Could not refresh import status: ${errorMessage}. Checking again…`,
            });
            return next;
          });
        }
      }
      polling = false;
    };

    const interval = setInterval(() => {
      void poll();
    }, 1000);

    void poll();

    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, [updateOperations]);

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
    }

    return result;
  }, [updateOperations]);

  const cancelBatchImport = useCallback(async (id: string): Promise<ApiResult<number>> => {
    const result = await VaultAPI.cancelBatchJob(id);
    if (result.ok) {
      // The job is cancelled, but each file's outcome stays whatever the
      // backend last reported: the file in flight may still finish indexing,
      // and polling continues until every item has settled.
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
