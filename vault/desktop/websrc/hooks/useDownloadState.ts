import { useMemo } from 'react';

import { useDownloadStore } from '../stores/downloadStore';

import type { DownloadStatus } from '../types/downloads';

export const useDownloadState = () => {
  const downloads = useDownloadStore((state) => state.downloads);
  const activeDownloads = useDownloadStore((state) => state.activeDownloads);
  const isDrawerOpen = useDownloadStore((state) => state.isDrawerOpen);
  const setDownload = useDownloadStore((state) => state.setDownload);
  const updateDownloadProgress = useDownloadStore((state) => state.updateDownloadProgress);
  const updateDownloadState = useDownloadStore((state) => state.updateDownloadState);
  const setDownloadError = useDownloadStore((state) => state.setDownloadError);
  const removeDownload = useDownloadStore((state) => state.removeDownload);
  const clearDownloads = useDownloadStore((state) => state.clearDownloads);
  const cleanupOldDownloads = useDownloadStore((state) => state.cleanupOldDownloads);
  const getDownload = useDownloadStore((state) => state.getDownload);
  const getTotalCount = useDownloadStore((state) => state.getTotalCount);
  const getActiveCount = useDownloadStore((state) => state.getActiveCount);
  const getActiveDownloads = useDownloadStore((state) => state.getActiveDownloads);
  const getAllDownloads = useDownloadStore((state) => state.getAllDownloads);
  const getDownloadsByModel = useDownloadStore((state) => state.getDownloadsByModel);
  const getActiveDownloadForModel = useDownloadStore((state) => state.getActiveDownloadForModel);
  const hasActiveDownloadForModel = useDownloadStore((state) => state.hasActiveDownloadForModel);
  const getLatestDownloadForModel = useDownloadStore((state) => state.getLatestDownloadForModel);
  const openDrawer = useDownloadStore((state) => state.openDrawer);
  const closeDrawer = useDownloadStore((state) => state.closeDrawer);
  const toggleDrawer = useDownloadStore((state) => state.toggleDrawer);

  const downloadsList = useMemo(() =>
    Array.from(downloads.values()),
    [downloads]
  );

  const activeDownloadsList = useMemo(() =>
    Array.from(activeDownloads)
      .map(id => downloads.get(id))
      .filter((d): d is DownloadStatus => d !== undefined),
    [activeDownloads, downloads]
  );

  const totalCount = useMemo(() => downloads.size, [downloads]);

  const activeCount = useMemo(() => activeDownloads.size, [activeDownloads]);

  return {
    downloads: downloadsList,
    activeDownloads: activeDownloadsList,
    downloadsMap: downloads,
    activeDownloadsSet: activeDownloads,
    isDrawerOpen,
    totalCount,
    activeCount,

    setDownload,
    updateDownloadProgress,
    updateDownloadState,
    setDownloadError,
    removeDownload,
    clearDownloads,
    cleanupOldDownloads,

    getDownload,
    getTotalCount,
    getActiveCount,
    getActiveDownloads,
    getAllDownloads,
    getDownloadsByModel,
    getActiveDownloadForModel,
    hasActiveDownloadForModel,
    getLatestDownloadForModel,

    openDrawer,
    closeDrawer,
    toggleDrawer,
  };
};
