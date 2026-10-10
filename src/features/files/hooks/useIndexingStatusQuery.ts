import { useEffect } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';

import VaultAPI from '@/lib/api';
import type { IndexStatus } from '@/types';
import type { IndexingActivity } from '@/types/api/files';
import type { IndexingSnapshot } from '@/types/api/indexing';

import { INDEXED_FOLDERS_QUERY_KEY } from './useIndexedFoldersQuery';

import type { UnlistenFn } from '@tauri-apps/api/event';

export const INDEXING_STATUS_QUERY_KEY = ['indexing', 'status'] as const;
export const INDEXING_ACTIVITIES_QUERY_KEY = ['indexing', 'activities'] as const;

const ACTIVE_STATUSES: ReadonlySet<IndexStatus> = new Set<IndexStatus>(['scanning', 'processing']);

const EMPTY_SNAPSHOT: IndexingSnapshot = {
  totalFiles: 0,
  processed: 0,
  failed: 0,
  status: 'idle',
  percentage: 0,
  paused: false,
  failures: [],
};

/** True while the index is scanning or processing. */
export function isIndexingActive(snapshot: IndexingSnapshot | undefined): boolean {
  return snapshot ? ACTIVE_STATUSES.has(snapshot.status) : false;
}

/**
 * The index's live state. The `indexing://progress` event is the mechanism; the
 * poll is a safety net for the case where the app misses an event (the broadcast
 * channel drops when a subscriber lags). Polling therefore runs at 2s only while
 * a run is active and stops entirely when it isn't — an idle vault costs nothing.
 */
export function useIndexingStatusQuery() {
  const queryClient = useQueryClient();

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let mounted = true;

    void listen<Partial<IndexingSnapshot>>('indexing://progress', (event) => {
      // The event payload is the engine's own progress struct: camelCase, but
      // without `paused` or `failures`. Merge rather than replace, or the
      // failure list blinks empty on every progress tick.
      queryClient.setQueryData<IndexingSnapshot>(INDEXING_STATUS_QUERY_KEY, (prev) => {
        const base = prev ?? EMPTY_SNAPSHOT;
        return {
          ...base,
          ...event.payload,
          paused: event.payload.paused ?? base.paused,
          failures: event.payload.failures ?? base.failures,
        };
      });
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [queryClient]);

  return useQuery<IndexingSnapshot>({
    queryKey: INDEXING_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getIndexProgress();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    refetchInterval: (query) => (isIndexingActive(query.state.data) ? 2000 : false),
    refetchIntervalInBackground: false,
    staleTime: 1000,
  });
}

/** Recent indexing activity. Only mounted where it is shown (popover, Settings). */
export function useIndexingActivitiesQuery(limit = 25, enabled = true) {
  return useQuery<IndexingActivity[]>({
    queryKey: [...INDEXING_ACTIVITIES_QUERY_KEY, limit],
    queryFn: async () => {
      const result = await VaultAPI.getIndexingActivities(limit);
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    enabled,
    staleTime: 15_000,
  });
}

export type IndexingControlAction = 'pause' | 'resume' | 'cancel';

/** Pause / Resume / Stop. Each invalidates the status query on settle. */
export function useIndexingControlMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, IndexingControlAction>({
    mutationFn: async (action) => {
      const result =
        action === 'pause'
          ? await VaultAPI.pauseIndexing()
          : action === 'resume'
            ? await VaultAPI.resumeIndexing()
            : await VaultAPI.cancelIndexing();
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: INDEXING_STATUS_QUERY_KEY });
    },
  });
}

/**
 * Forget one entry in the run's failure list.
 *
 * The list is engine state read back by `getIndexProgress`, so a row the user
 * has dealt with has to be cleared there — a local `useState` dismissal
 * reappears as soon as another view reads the snapshot. Best effort: an older
 * backend without the command, or a path that has already aged out, must not
 * turn a successful retry or removal into an error.
 */
async function clearFailureQuietly(path: string): Promise<void> {
  try {
    await VaultAPI.clearIndexingFailure(path);
  } catch {
    // The file is retried or removed either way; the row is what's left.
  }
}

/**
 * Retry one failed file. Tries `indexFile` first — a file that failed has no
 * `documents` row (the index transaction rolls it back), so `reindexFile` would
 * return NotFound. Falls back to `reindexFile` when the path is already indexed.
 */
export function useRetryIndexedFileMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: async (path) => {
      const indexed = await VaultAPI.indexFile(path);
      if (indexed.ok) {
        await clearFailureQuietly(path);
        return;
      }
      const reindexed = await VaultAPI.reindexFile(path);
      if (!reindexed.ok) {
        throw new Error(reindexed.error || indexed.error);
      }
      await clearFailureQuietly(path);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: INDEXING_STATUS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: INDEXING_ACTIVITIES_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: INDEXED_FOLDERS_QUERY_KEY });
    },
  });
}

/** Drop a file from the index and from the failure list. */
export function useRemoveIndexedFileMutation() {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: async (path) => {
      const result = await VaultAPI.removeIndexedFile(path);
      if (!result.ok) {
        throw new Error(result.error);
      }
      await clearFailureQuietly(path);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: INDEXING_STATUS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: INDEXING_ACTIVITIES_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: INDEXED_FOLDERS_QUERY_KEY });
    },
  });
}
