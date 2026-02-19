import { useEffect, useState, useCallback } from 'react';

import { z } from 'zod';

import { TauriEventNames, EventSchemas, TauriEvents, listenValidated } from '@/types/events';

import VaultAPI from '../lib/api';
import { useDownloadStore } from '../stores/downloadStore';

import type { DownloadStatus, DownloadState } from '../types/downloads';

const STATUS_TO_STORE_STATE: Record<string, DownloadState> = {
  pending: 'Pending',
  downloading: 'Downloading',
  paused: 'Paused',
  completed: 'Completed',
  error: 'Failed',
  cancelled: 'Cancelled',
  failed: 'Failed',
};

const toStoreState = (status: string): DownloadState =>
  STATUS_TO_STORE_STATE[status.toLowerCase()] ?? 'Pending';

const toSnapshotState = (state: string): TauriEvents.Downloads.Single['status'] => {
  const normalized = state.toLowerCase();
  if (normalized === 'failed') return 'error';
  if (normalized === 'pending' || normalized === 'downloading' || normalized === 'paused' || normalized === 'completed' || normalized === 'error' || normalized === 'cancelled') {
    return normalized;
  }
  return 'pending';
};

const toDownloadStatus = (id: string, snapshot: TauriEvents.Downloads.Single, modelId?: string): DownloadStatus => ({
  id,
  url: '',
  destination: snapshot.filename,
  state: toStoreState(snapshot.status),
  bytes_downloaded: snapshot.bytesDownloaded,
  total_bytes: snapshot.totalBytes,
  bytes_per_second: snapshot.bytesPerSecond,
  percentage: snapshot.percentage,
  eta_seconds: snapshot.etaSeconds,
  error_message: null,
  retry_count: 0,
  created_at: new Date().toISOString(),
  started_at: null,
  completed_at: snapshot.status === 'completed' || snapshot.status === 'error' || snapshot.status === 'cancelled' ? new Date().toISOString() : null,
  model_id: modelId,
  model_name: modelId,
});

export function useDownloads() {
  const [downloads, setDownloads] = useState<Map<string, TauriEvents.Downloads.StateSnapshot>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const setStoreDownload = useDownloadStore((state) => state.setDownload);
  const setStoreError = useDownloadStore((state) => state.setDownloadError);
  const removeStoreDownload = useDownloadStore((state) => state.removeDownload);

  useEffect(() => {
    let mounted = true;
    let unlistenProgress: (() => void) | null = null;
    let unlistenFailed: (() => void) | null = null;

    (async () => {
      try {
        // Hydrate drawer with existing sessions so users can open it
        // mid-download and still see current work.
        const existing = await VaultAPI.listDownloads();
        if (existing.ok && mounted) {
          const seeded = new Map<string, TauriEvents.Downloads.StateSnapshot>();
          for (const item of existing.data) {
            const status = toSnapshotState(item.state);
            const snapshot: TauriEvents.Downloads.Single = {
              kind: 'single',
              id: item.id,
              filename: item.destination.split(/[\\/]/).pop() ?? 'unknown',
              bytesDownloaded: item.bytes_downloaded,
              totalBytes: item.total_bytes,
              bytesPerSecond: item.bytes_per_second,
              percentage: item.percentage,
              etaSeconds: item.eta_seconds,
              status,
            };
            seeded.set(item.id, snapshot);
            setStoreDownload(item.id, {
              ...item,
              state: toStoreState(item.state),
            });
          }
          setDownloads(seeded);
        }

        // Listen for download progress events
        const unlistenProgressFn = await listenValidated(
          TauriEventNames.Downloads.Progress,
          EventSchemas.Downloads.StateSnapshot,
          (event: { payload: EventSchemas.Downloads.StateSnapshot }) => {
            if (!mounted) return;

            const snapshot = event.payload;

            setDownloads((prev) => {
              const next = new Map(prev);

              // Update or add the snapshot
              const id = snapshot.kind === 'single' ? snapshot.id : snapshot.id;
              next.set(id, snapshot);

               if (snapshot.kind === 'single') {
                 setStoreDownload(snapshot.id, toDownloadStatus(snapshot.id, snapshot));
               } else {
                 // Represent batch snapshots as synthetic per-file entries in store
                 for (const file of snapshot.files) {
                   const syntheticId = `${snapshot.id}:${file.filename}`;
                   const fileSnapshot: TauriEvents.Downloads.Single = {
                     kind: 'single',
                     id: syntheticId,
                     filename: file.filename,
                     bytesDownloaded: file.bytesDownloaded,
                     totalBytes: file.totalBytes,
                     bytesPerSecond: snapshot.aggregateBytesPerSecond,
                     percentage: file.totalBytes > 0 ? (file.bytesDownloaded / file.totalBytes) * 100 : 0,
                     etaSeconds: snapshot.aggregateEtaSeconds,
                     status: file.status,
                   };
                   next.set(syntheticId, fileSnapshot);
                   setStoreDownload(
                     syntheticId,
                     toDownloadStatus(syntheticId, fileSnapshot, snapshot.id)
                   );
                 }
               }

              // Remove completed downloads after 5 seconds
              if (snapshot.status === 'completed') {
                setTimeout(() => {
                  if (mounted) {
                    setDownloads((current) => {
                      const updated = new Map(current);
                      updated.delete(id);
                      removeStoreDownload(id);
                      return updated;
                    });
                  }
                }, 5000);
              }

              return next;
            });
          },
          (error: z.ZodError) => {
            if (!mounted) return;
            console.error('[useDownloads] Validation error:', error.format());
            setError('Invalid download event received from backend');
          }
        );

        // Listen for download failed events
        const unlistenFailedFn = await listenValidated(
          TauriEventNames.Downloads.Failed,
          EventSchemas.Downloads.Failed,
          (event: { payload: EventSchemas.Downloads.Failed }) => {
            if (!mounted) return;

            const { id, error: errorMsg } = event.payload;
            console.error(`[useDownloads] Download ${id} failed:`, errorMsg);
            setError(`Download failed: ${errorMsg}`);
            setStoreError(id, errorMsg);

            // Remove the failed download from state after showing error
            setTimeout(() => {
              if (mounted) {
                setDownloads((current) => {
                  const updated = new Map(current);
                  updated.delete(id);
                  removeStoreDownload(id);
                  return updated;
                });
              }
            }, 10000); // Keep failed downloads visible for 10 seconds
          },
          (error: z.ZodError) => {
            if (!mounted) return;
            console.error('[useDownloads] Failed event validation error:', error.format());
          }
        );

        if (mounted) {
          unlistenProgress = unlistenProgressFn;
          unlistenFailed = unlistenFailedFn;
        } else {
          // Component unmounted before listeners were set up
          unlistenProgressFn();
          unlistenFailedFn();
        }
      } catch (err) {
        if (!mounted) return;
        console.error('[useDownloads] Setup error:', err);
        setError(err instanceof Error ? err.message : 'Failed to setup download listener');
      }
    })();

    return () => {
      mounted = false;
      unlistenProgress?.();
      unlistenFailed?.();
    };
  }, []);

  const pauseDownload = useCallback(async (id: string): Promise<void> => {
    const result = await VaultAPI.pauseDownload(id);
    if (!result.ok) {
      throw new Error(`Failed to pause download: ${result.error}`);
    }
  }, []);

  const resumeDownload = useCallback(async (id: string): Promise<void> => {
    const result = await VaultAPI.resumeDownload(id);
    if (!result.ok) {
      throw new Error(`Failed to resume download: ${result.error}`);
    }
  }, []);

  const cancelDownload = useCallback(async (id: string): Promise<void> => {
    try {
      const result = await VaultAPI.cancelDownload(id);
      if (!result.ok) {
        throw new Error(result.error);
      }
    } catch (error) {
      console.error('[useDownloads] Failed to cancel download:', error);
      throw new Error(`Failed to cancel download: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, []);

  const retryDownload = useCallback(async (id: string): Promise<void> => {
    const result = await VaultAPI.retryDownload(id);
    if (!result.ok) {
      throw new Error(`Failed to retry download: ${result.error}`);
    }
  }, []);

  const removeDownload = useCallback(async (id: string): Promise<void> => {
    const result = await VaultAPI.removeDownload(id);
    if (result.ok) {
      setDownloads((prev) => {
        const next = new Map(prev);
        next.delete(id);
        return next;
      });
    } else {
      throw new Error(`Failed to remove download: ${result.error}`);
    }
  }, []);

  const clearCompletedDownloads = useCallback(async (): Promise<number> => {
    const result = await VaultAPI.clearCompletedDownloads();
    if (result.ok) {
      setDownloads((prev) => {
        const next = new Map(prev);
        for (const [id, download] of prev.entries()) {
          if (download.status === 'completed') {
            next.delete(id);
          }
        }
        return next;
      });
      return result.data;
    } else {
      throw new Error(`Failed to clear completed downloads: ${result.error}`);
    }
  }, []);

  const fetchAllDownloads = useCallback(async (): Promise<void> => {
    const existing = await VaultAPI.listDownloads();
    if (!existing.ok) {
      throw new Error(existing.error);
    }

    const seeded = new Map<string, TauriEvents.Downloads.StateSnapshot>();
    for (const item of existing.data) {
      const snapshot: TauriEvents.Downloads.Single = {
        kind: 'single',
        id: item.id,
        filename: item.destination.split(/[\\/]/).pop() ?? 'unknown',
        bytesDownloaded: item.bytes_downloaded,
        totalBytes: item.total_bytes,
        bytesPerSecond: item.bytes_per_second,
        percentage: item.percentage,
        etaSeconds: item.eta_seconds,
        status: toSnapshotState(item.state),
      };
      seeded.set(item.id, snapshot);
      setStoreDownload(item.id, {
        ...item,
        state: toStoreState(item.state),
      });
    }
    setDownloads(seeded);
  }, [setStoreDownload]);

  const getDownload = useCallback((id: string): TauriEvents.Downloads.StateSnapshot | undefined => downloads.get(id), [downloads]);

  return {
    downloads: Array.from(downloads.values()),
    error,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    retryDownload,
    removeDownload,
    clearCompletedDownloads,
    fetchAllDownloads,
    getDownload,
  };
}
