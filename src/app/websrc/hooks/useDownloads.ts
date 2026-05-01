import { useEffect, useCallback } from 'react';

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

interface SnapshotIdentity {
  modelId?: string;
  modelName?: string;
  existing?: DownloadStatus;
}

const toDownloadStatus = (
  id: string,
  snapshot: TauriEvents.Downloads.Single,
  identity: SnapshotIdentity = {}
): DownloadStatus => {
  const now = new Date().toISOString();
  const modelId = identity.modelId ?? identity.existing?.model_id;
  const modelName =
    identity.modelName ??
    identity.existing?.model_name ??
    modelId ??
    snapshot.filename;
  const isTerminal =
    snapshot.status === 'completed' ||
    snapshot.status === 'error' ||
    snapshot.status === 'cancelled';

  return {
    id,
    url: identity.existing?.url || snapshot.filename,
    destination: identity.existing?.destination || snapshot.filename,
    state: toStoreState(snapshot.status),
    bytes_downloaded: snapshot.bytesDownloaded,
    total_bytes: snapshot.totalBytes,
    bytes_per_second: snapshot.bytesPerSecond,
    percentage: snapshot.percentage,
    eta_seconds: snapshot.etaSeconds,
    error_message: identity.existing?.error_message ?? null,
    retry_count: identity.existing?.retry_count ?? 0,
    created_at: identity.existing?.created_at ?? now,
    started_at: identity.existing?.started_at ?? null,
    completed_at: isTerminal ? (identity.existing?.completed_at ?? now) : null,
    model_id: modelId,
    model_name: modelName,
  };
};


export function useDownloadsListener(): void {
  useEffect(() => {
    let mounted = true;
    let unlistenProgress: (() => void) | null = null;
    let unlistenFailed: (() => void) | null = null;

    const setStoreDownload = useDownloadStore.getState().setDownload;
    const setStoreError = useDownloadStore.getState().setDownloadError;
    const removeStoreDownload = useDownloadStore.getState().removeDownload;
    const setListenerError = useDownloadStore.getState().setListenerError;

    (async () => {
      try {
        const existing = await VaultAPI.listDownloads();
        if (existing.ok && mounted) {
          const liveStoreKeys = new Set<string>();
          for (const item of existing.data) {
            const filename = item.destination.split(/[\\/]/).pop() ?? 'unknown';
            const storeKey = item.model_id
              ? `${item.model_id}:${filename}`
              : item.id;
            liveStoreKeys.add(storeKey);
            setStoreDownload(storeKey, {
              ...item,
              state: toStoreState(item.state),
            });
          }
          // Drop any stale store entries the backend no longer reports.
          const storeDownloads = useDownloadStore.getState().downloads;
          for (const id of Array.from(storeDownloads.keys())) {
            if (!liveStoreKeys.has(id)) {
              removeStoreDownload(id);
            }
          }
        }

        const unlistenProgressFn = await listenValidated(
          TauriEventNames.Downloads.Progress,
          EventSchemas.Downloads.StateSnapshot,
          (event: { payload: EventSchemas.Downloads.StateSnapshot }) => {
            if (!mounted) return;
            const snapshot = event.payload;

            if (snapshot.kind === 'single') {
              const existing = useDownloadStore.getState().getDownload(snapshot.id);
              setStoreDownload(
                snapshot.id,
                toDownloadStatus(snapshot.id, snapshot, { existing })
              );
            } else {
              for (const file of snapshot.files) {
                const syntheticId = `${snapshot.id}:${file.filename}`;
                const fileSnapshot: TauriEvents.Downloads.Single = {
                  kind: 'single',
                  id: syntheticId,
                  filename: file.filename,
                  bytesDownloaded: file.bytesDownloaded,
                  totalBytes: file.totalBytes,
                  bytesPerSecond: snapshot.aggregateBytesPerSecond,
                  percentage:
                    file.totalBytes > 0
                      ? (file.bytesDownloaded / file.totalBytes) * 100
                      : 0,
                  etaSeconds: snapshot.aggregateEtaSeconds,
                  status: file.status,
                };
                const existing = useDownloadStore.getState().getDownload(syntheticId);
                setStoreDownload(
                  syntheticId,
                  toDownloadStatus(syntheticId, fileSnapshot, {
                    modelId: snapshot.id,
                    modelName: snapshot.groupName,
                    existing,
                  })
                );
              }
            }
          },
          (error: z.ZodError) => {
            if (!mounted) return;
            console.error('[useDownloadsListener] Validation error:', error.format());
            setListenerError('Invalid download event received from backend');
          }
        );

        const unlistenFailedFn = await listenValidated(
          TauriEventNames.Downloads.Failed,
          EventSchemas.Downloads.Failed,
          (event: { payload: EventSchemas.Downloads.Failed }) => {
            if (!mounted) return;
            const { id, error: errorMsg } = event.payload;
            console.error(`[useDownloadsListener] Download ${id} failed:`, errorMsg);
            setListenerError(`Download failed: ${errorMsg}`);
            setStoreError(id, errorMsg);
            // Failed entries persist in the store until either the user
            // hits Retry or Remove, or the 24h cleanup runs. The drawer
            // shows the error message + Retry button while it's there.
          },
          (error: z.ZodError) => {
            if (!mounted) return;
            console.error('[useDownloadsListener] Failed event validation error:', error.format());
          }
        );

        if (mounted) {
          unlistenProgress = unlistenProgressFn;
          unlistenFailed = unlistenFailedFn;
        } else {
          unlistenProgressFn();
          unlistenFailedFn();
        }
      } catch (err) {
        if (!mounted) return;
        console.error('[useDownloadsListener] Setup error:', err);
        setListenerError(err instanceof Error ? err.message : 'Failed to setup download listener');
      }
    })();

    return () => {
      mounted = false;
      unlistenProgress?.();
      unlistenFailed?.();
    };
  }, []);
}

export function useDownloadActions() {
  const removeStoreDownload = useDownloadStore((s) => s.removeDownload);

  const resyncFromBackend = useCallback(async () => {
    const existing = await VaultAPI.listDownloads();
    if (!existing.ok) return;
    const setStoreDownload = useDownloadStore.getState().setDownload;
    const liveStoreKeys = new Set<string>();
    for (const item of existing.data) {
      const filename = item.destination.split(/[\\/]/).pop() ?? 'unknown';
      const storeKey = item.model_id ? `${item.model_id}:${filename}` : item.id;
      liveStoreKeys.add(storeKey);
      setStoreDownload(storeKey, {
        ...item,
        state: toStoreState(item.state),
      });
    }
    const storeDownloads = useDownloadStore.getState().downloads;
    for (const id of Array.from(storeDownloads.keys())) {
      if (!liveStoreKeys.has(id)) {
        removeStoreDownload(id);
      }
    }
  }, [removeStoreDownload]);

  const isStaleSessionError = (msg: string): boolean => {
    const e = msg.toLowerCase();
    return e.includes('not found') || e.includes('already removed');
  };

  const resolveBackendId = (storeKey: string): string => {
    const row = useDownloadStore.getState().downloads.get(storeKey);
    if (row) return row.id;
    return storeKey.includes(':') ? storeKey.split(':')[0] : storeKey;
  };

  const pauseDownload = useCallback(
    async (id: string): Promise<void> => {
      const result = await VaultAPI.pauseDownload(resolveBackendId(id));
      if (result.ok) return;
      if (isStaleSessionError(result.error)) {
        await resyncFromBackend();
        return;
      }
      throw new Error(`Failed to pause download: ${result.error}`);
    },
    [resyncFromBackend]
  );

  const resumeDownload = useCallback(
    async (id: string): Promise<void> => {
      const result = await VaultAPI.resumeDownload(resolveBackendId(id));
      if (result.ok) return;
      if (isStaleSessionError(result.error)) {
        await resyncFromBackend();
        return;
      }
      throw new Error(`Failed to resume download: ${result.error}`);
    },
    [resyncFromBackend]
  );

  const cancelDownload = useCallback(
    async (id: string): Promise<void> => {
      const result = await VaultAPI.cancelDownload(resolveBackendId(id));
      if (result.ok) return;
      if (isStaleSessionError(result.error)) {
        await resyncFromBackend();
        return;
      }
      throw new Error(`Failed to cancel download: ${result.error}`);
    },
    [resyncFromBackend]
  );

  const retryDownload = useCallback(
    async (id: string): Promise<void> => {
      const result = await VaultAPI.retryDownload(resolveBackendId(id));
      if (result.ok) return;
      if (isStaleSessionError(result.error)) {
        await resyncFromBackend();
        return;
      }
      throw new Error(`Failed to retry download: ${result.error}`);
    },
    [resyncFromBackend]
  );

  const removeDownload = useCallback(
    async (id: string): Promise<void> => {
      const backendId = resolveBackendId(id);
      const removeFromStore = () => {
        removeStoreDownload(id);
      };

      const result = await VaultAPI.removeDownload(backendId);
      if (result.ok) {
        removeFromStore();
        return;
      }

      if (isStaleSessionError(result.error)) {
        removeFromStore();
        return;
      }

      throw new Error(`Failed to remove download: ${result.error}`);
    },
    [removeStoreDownload]
  );

  const clearCompletedDownloads = useCallback(async (): Promise<number> => {
    const result = await VaultAPI.clearCompletedDownloads();
    if (!result.ok) throw new Error(`Failed to clear completed downloads: ${result.error}`);
    const all = useDownloadStore.getState().downloads;
    for (const [id, download] of all.entries()) {
      if (download.state === 'Completed') removeStoreDownload(id);
    }
    return result.data;
  }, [removeStoreDownload]);

  const fetchAllDownloads = useCallback(async (): Promise<void> => {
    const existing = await VaultAPI.listDownloads();
    if (!existing.ok) throw new Error(existing.error);

    const setStoreDownload = useDownloadStore.getState().setDownload;
    const liveStoreKeys = new Set<string>();
    for (const item of existing.data) {
      const filename = item.destination.split(/[\\/]/).pop() ?? 'unknown';
      const storeKey = item.model_id ? `${item.model_id}:${filename}` : item.id;
      liveStoreKeys.add(storeKey);
      setStoreDownload(storeKey, {
        ...item,
        state: toStoreState(item.state),
      });
    }
    const storeDownloads = useDownloadStore.getState().downloads;
    for (const id of Array.from(storeDownloads.keys())) {
      if (!liveStoreKeys.has(id)) removeStoreDownload(id);
    }
  }, [removeStoreDownload]);

  return {
    pauseDownload,
    resumeDownload,
    cancelDownload,
    retryDownload,
    removeDownload,
    clearCompletedDownloads,
    fetchAllDownloads,
  };
}
