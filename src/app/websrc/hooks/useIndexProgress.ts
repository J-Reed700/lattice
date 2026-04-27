import { useState, useEffect, useCallback } from 'react';

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { VaultAPI } from '../lib/api';

interface IndexProgress {
  status: 'idle' | 'scanning' | 'processing' | 'complete' | 'error' | 'cancelled';
  currentFile?: string;
  processed: number;
  totalFiles: number;
  failed: number;
  percentage: number;
  error?: string;
  estimatedRemainingMs?: number;
}

interface UseIndexProgressOptions {
  onComplete?: () => void;
  onError?: (error: string) => void;
  onCancel?: () => void;
}

interface UseIndexProgressReturn {
  progress: IndexProgress;
  error: string | null;
  isActive: boolean;
  handleCancel: () => Promise<void>;
}

interface IndexingStartedEvent {
  path: string;
  total_files?: number;
}

interface IndexingProgressEvent {
  current: number;
  total: number;
  filename: string;
}

interface IndexingCompleteEvent {
  path: string;
  indexed_count: number;
  duration_ms?: number;
}

interface IndexingErrorEvent {
  message: string;
  path: string;
  filename?: string;
}

export function useIndexProgress(options: UseIndexProgressOptions = {}): UseIndexProgressReturn {
  const { onComplete, onError, onCancel } = options;

  const [progress, setProgress] = useState<IndexProgress>({
    status: 'idle',
    processed: 0,
    totalFiles: 0,
    failed: 0,
    percentage: 0,
  });

  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlistenProgress: UnlistenFn | null = null;
    let unlistenStarted: UnlistenFn | null = null;
    let unlistenComplete: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;

    const setupListener = async () => {
      const initial = await VaultAPI.getIndexProgress();
      if (initial.ok) {
        setProgress((prev) => ({
          ...prev,
          status: initial.data.status,
          currentFile: initial.data.currentFile,
          processed: initial.data.processed,
          totalFiles: initial.data.totalFiles,
          failed: initial.data.failed ?? 0,
          percentage: initial.data.percentage,
          estimatedRemainingMs: initial.data.estimatedRemainingMs,
        }));
      }

      unlistenStarted = await listen<IndexingStartedEvent>('indexing-started', (event) => {
        const { total_files: totalFiles } = event.payload;
        setProgress((prev) => ({
          ...prev,
          status: 'scanning',
          totalFiles: totalFiles ?? prev.totalFiles,
          percentage: 0,
        }));
      });

      unlistenProgress = await listen<IndexingProgressEvent>('indexing-progress', (event) => {
        const { current, total, filename } = event.payload;
        const percentage = total > 0 ? (current / total) * 100 : 0;
        setProgress((prev) => ({
          ...prev,
          status: 'processing',
          processed: current,
          totalFiles: total,
          failed: prev.failed,
          currentFile: filename,
          percentage,
        }));
      });

      unlistenError = await listen<IndexingErrorEvent>('indexing-error', (event) => {
        const { message } = event.payload;
        setProgress((prev) => ({
          ...prev,
          status: 'error',
          error: message,
        }));
        setError(message);
        onError?.(message);
      });

      unlistenComplete = await listen<IndexingCompleteEvent>('indexing-complete', (event) => {
        const { indexed_count: indexedCount } = event.payload;
        setProgress((prev) => ({
          ...prev,
          status: 'complete',
          processed: indexedCount,
          totalFiles: indexedCount,
          failed: prev.failed,
          percentage: 100,
        }));
        onComplete?.();
      });
    };

    setupListener();

    return () => {
      unlistenProgress?.();
      unlistenStarted?.();
      unlistenComplete?.();
      unlistenError?.();
    };
  }, [onComplete, onError, onCancel]);

  const handleCancel = useCallback(async () => {
    try {
      const result = await VaultAPI.cancelIndexing();
      if (result.ok) {
        setProgress((prev) => ({ ...prev, status: 'cancelled' }));
        onCancel?.();
      } else {
        setError(result.error);
        onError?.(result.error);
      }
    } catch (err) {
      console.error('Failed to cancel indexing:', err);
    }
  }, [onCancel, onError]);

  const isActive = progress.status === 'scanning' || progress.status === 'processing';

  return {
    progress,
    error,
    isActive,
    handleCancel,
  };
}
