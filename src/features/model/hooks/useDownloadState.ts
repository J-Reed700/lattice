import { useCallback, useMemo } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import type { DownloadStatus, DownloadState } from '@/types/downloads';

import { DOWNLOADS_QUERY_KEY, fetchDownloadMap } from './useDownloads';
import { useDownloadStore } from '../stores/downloadStore';


const isActiveState = (state: DownloadState): boolean =>
  state === 'Downloading' || state === 'Pending' || state === 'Paused';

export type ModelDownloadSummary = DownloadStatus & { files?: DownloadStatus[] };

/**
 * Aggregates all session rows for a model into one catalog-friendly status,
 * preferring live/failed states over finished ones (`Downloading > Failed > Paused > Pending`)
 * so multi-file failures and paused transfers are never hidden by sibling files.
 */
export function summarizeModelDownloads(downloads: readonly DownloadStatus[]): ModelDownloadSummary | null {
  if (downloads.length === 0) return null;
  const nonCancelled = downloads.filter((download) => download.state !== 'Cancelled');
  if (nonCancelled.length === 0) return null;

  const unfinished = nonCancelled.filter((download) => download.state !== 'Completed');
  if (unfinished.length === 0) {
    return nonCancelled[0];
  }

  const representative = unfinished[0];
  if (nonCancelled.length === 1) {
    return representative;
  }

  const totalBytes = nonCancelled.reduce((sum, download) => sum + (download.total_bytes ?? 0), 0);
  const bytesDownloaded = nonCancelled.reduce((sum, download) => sum + download.bytes_downloaded, 0);
  const percentage = totalBytes > 0 ? (bytesDownloaded / totalBytes) * 100 : null;

  let state: DownloadState = 'Pending';
  let errorMessage: string | null = null;
  if (unfinished.some((download) => download.state === 'Downloading')) {
    state = 'Downloading';
  } else {
    const failed = unfinished.find((download) => download.state === 'Failed');
    if (failed) {
      state = 'Failed';
      errorMessage = failed.error_message ?? null;
    } else if (unfinished.some((download) => download.state === 'Paused')) {
      state = 'Paused';
    }
  }

  return {
    ...representative,
    state,
    bytes_downloaded: bytesDownloaded,
    total_bytes: totalBytes > 0 ? totalBytes : representative.total_bytes,
    percentage,
    error_message: errorMessage,
    files: [...nonCancelled],
  };
}

export const useDownloadState = () => {
  const api = useApi();
  const queryClient = useQueryClient();
  const downloadsQuery = useQuery<Map<string, DownloadStatus>>({
    queryKey: DOWNLOADS_QUERY_KEY,
    queryFn: () => fetchDownloadMap(api),
    staleTime: 15_000,
    // Events are low-latency hints; reconcile running rows from persisted
    // state if the bounded native event queue drops a terminal notification.
    refetchInterval: (query) => Array.from(query.state.data?.values() ?? [])
      .some((download) => download.state === 'Pending' || download.state === 'Downloading')
      ? 5_000 : false,
    refetchIntervalInBackground: false,
  });
  const downloadsMap = useMemo(
    () => downloadsQuery.data ?? new Map<string, DownloadStatus>(),
    [downloadsQuery.data]
  );
  const {
    isDrawerOpen,
    listenerError,
    setListenerError,
    openDrawer,
    closeDrawer,
    toggleDrawer,
  } = useDownloadStore();

  const updateCache = useCallback((
    updater: (current: Map<string, DownloadStatus>) => Map<string, DownloadStatus>
  ) => {
    queryClient.setQueryData<Map<string, DownloadStatus>>(
      DOWNLOADS_QUERY_KEY,
      (current) => updater(current ?? new Map())
    );
  }, [queryClient]);
  const setDownload = useCallback((id: string, status: DownloadStatus) => {
    updateCache((current) => new Map(current).set(id, status));
  }, [updateCache]);
  const updateDownloadProgress = useCallback((
    id: string,
    bytesDownloaded: number,
    bytesPerSecond: number,
    percentage: number | null,
    etaSeconds: number | null
  ) => {
    updateCache((current) => {
      const download = current.get(id);
      if (!download) return current;
      const next = new Map(current);
      next.set(id, {
        ...download,
        bytes_downloaded: bytesDownloaded,
        bytes_per_second: bytesPerSecond,
        percentage,
        eta_seconds: etaSeconds,
      });
      return next;
    });
  }, [updateCache]);
  const updateDownloadState = useCallback((id: string, state: DownloadState) => {
    updateCache((current) => {
      const download = current.get(id);
      if (!download) return current;
      const next = new Map(current);
      next.set(id, {
        ...download,
        state,
        completed_at: (state === 'Completed' || state === 'Failed')
          ? (download.completed_at ?? new Date().toISOString())
          : download.completed_at,
      });
      return next;
    });
  }, [updateCache]);
  const setDownloadError = useCallback((id: string, error: string) => {
    updateCache((current) => {
      const download = current.get(id);
      if (!download) return current;
      const next = new Map(current);
      next.set(id, {
        ...download,
        state: 'Failed',
        error_message: error,
        completed_at: download.completed_at ?? new Date().toISOString(),
      });
      return next;
    });
  }, [updateCache]);
  const removeDownload = useCallback((id: string) => {
    updateCache((current) => {
      const next = new Map(current);
      next.delete(id);
      return next;
    });
  }, [updateCache]);
  const clearDownloads = useCallback(() => {
    queryClient.setQueryData(DOWNLOADS_QUERY_KEY, new Map<string, DownloadStatus>());
  }, [queryClient]);
  const cleanupOldDownloads = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: DOWNLOADS_QUERY_KEY });
  }, [queryClient]);

  const downloads = useMemo(() => Array.from(downloadsMap.values()), [downloadsMap]);
  const activeDownloadEntries = useMemo(
    () => Array.from(downloadsMap.entries()).filter(([, download]) => isActiveState(download.state)),
    [downloadsMap]
  );
  const activeDownloads = useMemo(
    () => activeDownloadEntries.map(([, download]) => download),
    [activeDownloadEntries]
  );
  const activeDownloadsSet = useMemo(
    () => new Set(activeDownloadEntries.map(([id]) => id)),
    [activeDownloadEntries]
  );
  const getDownload = useCallback((id: string) => downloadsMap.get(id), [downloadsMap]);
  const getDownloadsByModel = useCallback(
    (modelId: string) => downloads.filter((download) => download.model_id === modelId),
    [downloads]
  );
  const getActiveDownloadForModel = useCallback(
    (modelId: string) => activeDownloads.find((download) => download.model_id === modelId) ?? null,
    [activeDownloads]
  );
  const getModelDownloadSummary = useCallback(
    (modelId: string) => summarizeModelDownloads(getDownloadsByModel(modelId)),
    [getDownloadsByModel]
  );
  const getLatestDownloadForModel = useCallback((modelId: string) => {
    const matches = downloads.filter((download) => download.model_id === modelId);
    return matches.reduce<DownloadStatus | null>((latest, current) => {
      if (!latest) return current;
      const latestTime = new Date(latest.started_at ?? latest.created_at).getTime();
      const currentTime = new Date(current.started_at ?? current.created_at).getTime();
      return currentTime > latestTime ? current : latest;
    }, null);
  }, [downloads]);

  return {
    downloads,
    activeDownloads,
    downloadsMap,
    activeDownloadsSet,
    isDrawerOpen,
    totalCount: downloads.length,
    activeCount: activeDownloads.length,
    listenerError,
    setListenerError,
    isLoading: downloadsQuery.isLoading,
    // The list has answered at least once; a row must not offer a download before.
    downloadsSettled: !downloadsQuery.isPending,
    error: downloadsQuery.error?.message ?? null,
    setDownload,
    updateDownloadProgress,
    updateDownloadState,
    setDownloadError,
    removeDownload,
    clearDownloads,
    cleanupOldDownloads,
    getDownload,
    getTotalCount: () => downloads.length,
    getActiveCount: () => activeDownloads.length,
    getActiveDownloads: () => activeDownloads,
    getAllDownloads: () => downloads,
    getDownloadsByModel,
    getActiveDownloadForModel,
    getModelDownloadSummary,
    hasActiveDownloadForModel: (modelId: string) => getActiveDownloadForModel(modelId) !== null,
    getLatestDownloadForModel,
    openDrawer,
    closeDrawer,
    toggleDrawer,
  };
};
