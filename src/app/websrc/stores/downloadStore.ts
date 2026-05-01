import { create } from 'zustand';

import type { DownloadStatus, DownloadState } from '../types/downloads';

const isActiveState = (state: DownloadState): boolean =>
  state === 'Downloading' || state === 'Pending' || state === 'Paused';

interface DownloadStore {
  downloads: Map<string, DownloadStatus>;
  activeDownloads: Set<string>;
  isDrawerOpen: boolean;
  listenerError: string | null;
  setListenerError: (error: string | null) => void;

  setDownload: (id: string, status: DownloadStatus) => void;
  updateDownloadProgress: (
    id: string,
    bytesDownloaded: number,
    bytesPerSecond: number,
    percentage: number | null,
    etaSeconds: number | null
  ) => void;
  updateDownloadState: (id: string, state: DownloadState) => void;
  setDownloadError: (id: string, error: string) => void;
  removeDownload: (id: string) => void;
  clearDownloads: () => void;
  cleanupOldDownloads: () => void;

  getDownload: (id: string) => DownloadStatus | undefined;
  getTotalCount: () => number;
  getActiveCount: () => number;
  getActiveDownloads: () => DownloadStatus[];
  getAllDownloads: () => DownloadStatus[];

  getDownloadsByModel: (modelId: string) => DownloadStatus[];
  getActiveDownloadForModel: (modelId: string) => DownloadStatus | null;
  hasActiveDownloadForModel: (modelId: string) => boolean;
  getLatestDownloadForModel: (modelId: string) => DownloadStatus | null;

  openDrawer: () => void;
  closeDrawer: () => void;
  toggleDrawer: () => void;
}

export const useDownloadStore = create<DownloadStore>((set, get) => ({
  downloads: new Map(),
  activeDownloads: new Set(),
  isDrawerOpen: false,
  listenerError: null,
  setListenerError: (error) => set({ listenerError: error }),

  setDownload: (id, status) => {
    console.log('[downloadStore] setDownload called:', id, 'model_id:', status.model_id, 'state:', status.state, 'url:', status.url);
    set((state) => {
      const newDownloads = new Map(state.downloads);
      newDownloads.set(id, status);
      console.log('[downloadStore] Total downloads in store after set:', newDownloads.size);

      const newActiveDownloads = new Set(state.activeDownloads);
      if (isActiveState(status.state)) {
        newActiveDownloads.add(id);
      } else {
        newActiveDownloads.delete(id);
      }

      return {
        downloads: newDownloads,
        activeDownloads: newActiveDownloads,
      };
    });
  },

  updateDownloadProgress: (id, bytesDownloaded, bytesPerSecond, percentage, etaSeconds) => {
    set((state) => {
      const download = state.downloads.get(id);
      if (!download) return state;

      const newDownloads = new Map(state.downloads);
      const updatedDownload = {
        ...download,
        bytes_downloaded: bytesDownloaded,
        bytes_per_second: bytesPerSecond,
        percentage,
        eta_seconds: etaSeconds,
      };

      // Set completed_at timestamp when state becomes Completed or Failed
      if ((download.state === 'Completed' || download.state === 'Failed') && !download.completed_at) {
        updatedDownload.completed_at = new Date().toISOString();
      }

      newDownloads.set(id, updatedDownload);

      return { downloads: newDownloads };
    });
  },

  updateDownloadState: (id, newState) => {
    set((state) => {
      const download = state.downloads.get(id);
      if (!download) return state;

      const newDownloads = new Map(state.downloads);
      const updatedDownload = {
        ...download,
        state: newState,
      };

      // Set completed_at timestamp when state becomes Completed or Failed
      if ((newState === 'Completed' || newState === 'Failed') && !download.completed_at) {
        updatedDownload.completed_at = new Date().toISOString();
      }

      newDownloads.set(id, updatedDownload);

      const newActiveDownloads = new Set(state.activeDownloads);
      if (isActiveState(newState)) {
        newActiveDownloads.add(id);
      } else {
        newActiveDownloads.delete(id);
      }

      return {
        downloads: newDownloads,
        activeDownloads: newActiveDownloads,
      };
    });
  },

  setDownloadError: (id, error) => {
    set((state) => {
      const download = state.downloads.get(id);
      if (!download) return state;

      const newDownloads = new Map(state.downloads);
      newDownloads.set(id, {
        ...download,
        state: 'Failed',
        error_message: error,
        completed_at: download.completed_at || new Date().toISOString(),
      });

      const newActiveDownloads = new Set(state.activeDownloads);
      newActiveDownloads.delete(id);

      return {
        downloads: newDownloads,
        activeDownloads: newActiveDownloads,
      };
    });
  },

  removeDownload: (id) => {
    set((state) => {
      const newDownloads = new Map(state.downloads);
      newDownloads.delete(id);

      const newActiveDownloads = new Set(state.activeDownloads);
      newActiveDownloads.delete(id);

      return {
        downloads: newDownloads,
        activeDownloads: newActiveDownloads,
      };
    });
  },

  clearDownloads: () => {
    set({ downloads: new Map(), activeDownloads: new Set() });
  },

  cleanupOldDownloads: () => {
    set((state) => {
      const now = Date.now();
      const TTL = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
      const newDownloads = new Map(state.downloads);
      const newActiveDownloads = new Set(state.activeDownloads);

      for (const [id, download] of newDownloads.entries()) {
        // Only cleanup completed/failed downloads with completed_at timestamp
        if ((download.state === 'Completed' || download.state === 'Failed') && download.completed_at) {
          const completedTime = new Date(download.completed_at).getTime();
          if (now - completedTime > TTL) {
            newDownloads.delete(id);
            newActiveDownloads.delete(id);
          }
        }
        // Never cleanup downloads with state 'Downloading' or 'Pending'
      }

      return {
        downloads: newDownloads,
        activeDownloads: newActiveDownloads,
      };
    });
  },

  getDownload: (id) => get().downloads.get(id),

  getTotalCount: () => get().downloads.size,

  getActiveCount: () => get().activeDownloads.size,

  getActiveDownloads: () => {
    const { downloads, activeDownloads } = get();
    return Array.from(activeDownloads)
      .map(id => downloads.get(id))
      .filter((d): d is DownloadStatus => d !== undefined);
  },

  getAllDownloads: () => Array.from(get().downloads.values()),

  getDownloadsByModel: (modelId) => {
    const { downloads } = get();
    return Array.from(downloads.values())
      .filter(d => d.model_id === modelId);
  },

  getActiveDownloadForModel: (modelId) => {
    const { downloads, activeDownloads } = get();
    for (const id of activeDownloads) {
      const download = downloads.get(id);
      if (download?.model_id === modelId) {
        return download;
      }
    }
    return null;
  },

  hasActiveDownloadForModel: (modelId) => {
    const { downloads, activeDownloads } = get();
    for (const id of activeDownloads) {
      const download = downloads.get(id);
      if (download?.model_id === modelId) {
        return true;
      }
    }
    return false;
  },

  getLatestDownloadForModel: (modelId) => {
    const { downloads } = get();
    const modelDownloads = Array.from(downloads.values())
      .filter(d => d.model_id === modelId);

    if (modelDownloads.length === 0) {
      return null;
    }

    return modelDownloads.reduce((latest, current) => {
      const latestTime = new Date(latest.started_at ?? latest.created_at).getTime();
      const currentTime = new Date(current.started_at ?? current.created_at).getTime();
      return currentTime > latestTime ? current : latest;
    });
  },

  openDrawer: () => {
    set({ isDrawerOpen: true });
  },

  closeDrawer: () => {
    set({ isDrawerOpen: false });
  },

  toggleDrawer: () => {
    set((state) => ({ isDrawerOpen: !state.isDrawerOpen }));
  },
}));

// Track cleanup timers to prevent duplicates during HMR
let cleanupTimeout: NodeJS.Timeout | null = null;
let cleanupInterval: NodeJS.Timeout | null = null;

/**
 * Start automatic cleanup of old downloads.
 * Call this once from App.tsx on mount.
 *
 * Prevents duplicate timers during Hot Module Reload by checking if interval exists.
 */
export const startDownloadCleanup = () => {
  // Prevent duplicate timers (critical for HMR)
  if (cleanupInterval) {
    console.log('[downloadStore] Cleanup already running, skipping initialization');
    return;
  }

  // Initial cleanup after 1 second
  cleanupTimeout = setTimeout(() => {
    useDownloadStore.getState().cleanupOldDownloads();
  }, 1000);

  // Recurring cleanup every hour
  cleanupInterval = setInterval(() => {
    useDownloadStore.getState().cleanupOldDownloads();
  }, 60 * 60 * 1000);

  console.log('[downloadStore] Cleanup timers started');
};

/**
 * Stop automatic cleanup (useful for testing/cleanup).
 */
export const stopDownloadCleanup = () => {
  if (cleanupTimeout) {
    clearTimeout(cleanupTimeout);
    cleanupTimeout = null;
  }
  if (cleanupInterval) {
    clearInterval(cleanupInterval);
    cleanupInterval = null;
  }
  console.log('[downloadStore] Cleanup timers stopped');
};
