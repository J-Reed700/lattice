import { useCallback, useEffect } from 'react';

import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { z } from 'zod';

import { TauriEventNames, EventSchemas, TauriEvents, listenValidated } from '@/types/events';

import VaultAPI from '../lib/api';
import { useDownloadStore } from '../stores/downloadStore';

import type { DownloadStatus, DownloadState } from '../types/downloads';

export const DOWNLOADS_QUERY_KEY = ['downloads'] as const;

const STATUS_TO_STATE: Record<string, DownloadState> = {
  pending: 'Pending',
  downloading: 'Downloading',
  paused: 'Paused',
  completed: 'Completed',
  error: 'Failed',
  cancelled: 'Cancelled',
  failed: 'Failed',
};

export const toDownloadState = (status: string): DownloadState =>
  STATUS_TO_STATE[status.toLowerCase()] ?? 'Pending';

const cacheKeyFor = (download: DownloadStatus): string => {
  const filename = download.destination.split(/[\\/]/).pop() ?? 'unknown';
  return download.model_id ? `${download.model_id}:${filename}` : download.id;
};

export async function fetchDownloadMap(): Promise<Map<string, DownloadStatus>> {
  const result = await VaultAPI.listDownloads();
  if (!result.ok) throw new Error(result.error);
  return new Map(result.data.map((download) => [
    cacheKeyFor(download),
    { ...download, state: toDownloadState(download.state) },
  ]));
}

function updateDownloadCache(
  queryClient: QueryClient,
  updater: (current: Map<string, DownloadStatus>) => Map<string, DownloadStatus>
) {
  queryClient.setQueryData<Map<string, DownloadStatus>>(
    DOWNLOADS_QUERY_KEY,
    (current) => updater(current ?? new Map())
  );
}

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
  const modelName = identity.modelName ?? identity.existing?.model_name ?? modelId ?? snapshot.filename;
  const isTerminal = ['completed', 'error', 'cancelled'].includes(snapshot.status);
  return {
    id,
    url: identity.existing?.url || snapshot.filename,
    destination: identity.existing?.destination || snapshot.filename,
    state: toDownloadState(snapshot.status),
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
    model_id: modelId ?? null,
    model_name: modelName,
  };
};

export function useDownloadsListener(): void {
  const queryClient = useQueryClient();
  const setListenerError = useDownloadStore((state) => state.setListenerError);

  useEffect(() => {
    let mounted = true;
    let unlistenProgress: (() => void) | null = null;
    let unlistenFailed: (() => void) | null = null;

    void (async () => {
      try {
        await queryClient.fetchQuery({ queryKey: DOWNLOADS_QUERY_KEY, queryFn: fetchDownloadMap });
        const progressListener = await listenValidated(
          TauriEventNames.Downloads.Event,
          EventSchemas.Downloads.StateSnapshot,
          (event: { payload: EventSchemas.Downloads.StateSnapshot }) => {
            if (!mounted) return;
            updateDownloadCache(queryClient, (current) => {
              const next = new Map(current);
              const snapshot = event.payload;
              if (snapshot.kind === 'single') {
                next.set(snapshot.id, toDownloadStatus(snapshot.id, snapshot, {
                  existing: current.get(snapshot.id),
                }));
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
                    percentage: file.totalBytes > 0
                      ? (file.bytesDownloaded / file.totalBytes) * 100
                      : 0,
                    etaSeconds: snapshot.aggregateEtaSeconds,
                    status: file.status,
                  };
                  next.set(syntheticId, toDownloadStatus(syntheticId, fileSnapshot, {
                    modelId: snapshot.id,
                    modelName: snapshot.groupName,
                    existing: current.get(syntheticId),
                  }));
                }
              }
              return next;
            });
          },
          (error: z.ZodError) => {
            if (!mounted) return;
            console.error('[useDownloadsListener] Validation error:', error.format());
            setListenerError('Invalid download event received from backend');
          }
        );
        const failedListener = await listenValidated(
          TauriEventNames.Downloads.Failed,
          EventSchemas.Downloads.Failed,
          (event: { payload: EventSchemas.Downloads.Failed }) => {
            if (!mounted) return;
            const { id, error } = event.payload;
            setListenerError(`Download failed: ${error}`);
            updateDownloadCache(queryClient, (current) => {
              const existing = current.get(id);
              if (!existing) return current;
              const next = new Map(current);
              next.set(id, {
                ...existing,
                state: 'Failed',
                error_message: error,
                completed_at: existing.completed_at ?? new Date().toISOString(),
              });
              return next;
            });
          },
          (error: z.ZodError) =>
            console.error('[useDownloadsListener] Failed event validation error:', error.format())
        );
        if (mounted) {
          unlistenProgress = progressListener;
          unlistenFailed = failedListener;
        } else {
          progressListener();
          failedListener();
        }
      } catch (error) {
        if (!mounted) return;
        console.error('[useDownloadsListener] Setup error:', error);
        setListenerError(error instanceof Error ? error.message : 'Failed to setup download listener');
      }
    })();

    return () => {
      mounted = false;
      unlistenProgress?.();
      unlistenFailed?.();
    };
  }, [queryClient, setListenerError]);
}

const isStaleSessionError = (message: string): boolean => {
  const normalized = message.toLowerCase();
  return normalized.includes('not found') || normalized.includes('already removed');
};

export function useDownloadActions() {
  const queryClient = useQueryClient();
  const downloads = queryClient.getQueryData<Map<string, DownloadStatus>>(DOWNLOADS_QUERY_KEY);
  const resolveBackendId = useCallback((storeKey: string): string => {
    const row = queryClient.getQueryData<Map<string, DownloadStatus>>(DOWNLOADS_QUERY_KEY)?.get(storeKey);
    return row?.id ?? (storeKey.includes(':') ? storeKey.split(':')[0] : storeKey);
  }, [queryClient]);
  const resyncFromBackend = useCallback(async () => {
    await queryClient.fetchQuery({ queryKey: DOWNLOADS_QUERY_KEY, queryFn: fetchDownloadMap, staleTime: 0 });
  }, [queryClient]);
  const runAction = useCallback(async (
    id: string,
    action: (backendId: string) => ReturnType<typeof VaultAPI.pauseDownload>,
    label: string
  ) => {
    const result = await action(resolveBackendId(id));
    if (!result.ok && !isStaleSessionError(result.error)) {
      throw new Error(`Failed to ${label} download: ${result.error}`);
    }
    await resyncFromBackend();
  }, [resolveBackendId, resyncFromBackend]);

  const pauseDownload = useCallback((id: string) =>
    runAction(id, VaultAPI.pauseDownload, 'pause'), [runAction]);
  const resumeDownload = useCallback((id: string) =>
    runAction(id, VaultAPI.resumeDownload, 'resume'), [runAction]);
  const cancelDownload = useCallback((id: string) =>
    runAction(id, VaultAPI.cancelDownload, 'cancel'), [runAction]);
  const retryDownload = useCallback((id: string) =>
    runAction(id, VaultAPI.retryDownload, 'retry'), [runAction]);
  const removeDownload = useCallback(async (id: string) => {
    const result = await VaultAPI.removeDownload(resolveBackendId(id));
    if (!result.ok && !isStaleSessionError(result.error)) {
      throw new Error(`Failed to remove download: ${result.error}`);
    }
    await resyncFromBackend();
  }, [resolveBackendId, resyncFromBackend]);
  const clearCompletedDownloads = useCallback(async (): Promise<number> => {
    const result = await VaultAPI.clearCompletedDownloads();
    if (!result.ok) throw new Error(`Failed to clear completed downloads: ${result.error}`);
    await resyncFromBackend();
    return result.data;
  }, [resyncFromBackend]);
  const fetchAllDownloads = useCallback(async (): Promise<void> => {
    await resyncFromBackend();
  }, [resyncFromBackend]);

  return {
    pauseDownload,
    resumeDownload,
    cancelDownload,
    retryDownload,
    removeDownload,
    clearCompletedDownloads,
    fetchAllDownloads,
    downloads,
  };
}
