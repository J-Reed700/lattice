import { useState, useEffect, useCallback, useRef } from 'react';

import { VaultAPI } from '../lib/api';

import type { ApiResult, BatchJobItem, BatchJobStatus } from '../types';

interface IndexingOperation {
  id: string;
  totalFiles: number;
  processedFiles: number;
  currentFile?: string;
  status: 'pending' | 'processing' | 'completed' | 'error';
  error?: string;
  items?: BatchJobItem[];
}

interface UseIndexingReturn {
  operations: Map<string, IndexingOperation>;
  isIndexing: boolean;
  startBatchImport: (filePaths: string[]) => Promise<ApiResult<string>>;
  getOperation: (id: string) => IndexingOperation | undefined;
  clearOperation: (id: string) => void;
}

export function useIndexing(): UseIndexingReturn {
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

  // Single event listener for all indexing operations
  useEffect(() => {
    let mounted = true;

    const poll = async () => {
      const activeOperations = Array.from(operationsRef.current.values()).filter(
        (op) => op.status === 'pending' || op.status === 'processing'
      );

      if (activeOperations.length === 0) {
        return;
      }

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
                status: 'error',
                error: result.error,
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

            next.set(operation.id, {
              id: operation.id,
              totalFiles: totalItems,
              processedFiles,
              status,
              error: existing?.error,
              items: result.data.items || existing?.items,
              currentFile: existing?.currentFile,
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
              status: 'error',
              error: errorMessage,
            });
            return next;
          });
        }
      }
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

  const startBatchImport = useCallback(async (filePaths: string[]): Promise<ApiResult<string>> => {
    const result = await VaultAPI.batchFileImport(filePaths);

    if (result.ok && result.data) {
      // Create pending operation
      updateOperations((prev) => {
        const next = new Map(prev);
        next.set(result.data, {
          id: result.data,
          totalFiles: filePaths.length,
          processedFiles: 0,
          status: 'pending',
        });
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
    startBatchImport,
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
      return 'completed';
    case 'failed':
    case 'error':
    case 'cancelled':
      return 'error';
    default:
      return 'processing';
  }
}
