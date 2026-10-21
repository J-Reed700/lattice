import { useCallback, useEffect, useRef } from 'react';

import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';

import { onJobStatus } from '@/features/jobs/api';
import type { ActivitySnapshotDto } from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import { unwrapApiResult } from '@/types/api/result';

export type { ActivityEntryDto, ActivitySnapshotDto } from '@/lib/bindings';

/** Moves when something with no event of its own changes: model requests queueing, models loading, the index turn order. */
export const ACTIVITY_CHANGED_EVENT = 'activity://changed';

/**
 * Every event after which the snapshot may read differently. Jobs come on
 * `jobs://status` through the shared jobs listener; downloads and the
 * library's indexer already publish their own.
 */
export const ACTIVITY_REFRESH_EVENTS = [
  ACTIVITY_CHANGED_EVENT,
  'download:progress',
  'download:completed',
  'download:failed',
  'indexing://progress',
] as const;

export const activityKeys = {
  snapshot: ['activity', 'snapshot'] as const,
};

export const activitySnapshotQueryOptions = () =>
  queryOptions({
    queryKey: activityKeys.snapshot,
    queryFn: async () => unwrapApiResult(await apiCall<ActivitySnapshotDto>('activity_snapshot')),
    // Kept current by the events below, not by refetching on a clock.
    staleTime: Infinity,
  });

/**
 * Everything running, paused or waiting, read again after every event that
 * may have moved it. Reads never overlap: events that land during a read are
 * answered by one more read after it. Mount it once; the returned `refresh`
 * reads again on demand (when the drawer opens).
 */
export function useActivitySnapshot() {
  const client = useQueryClient();
  const query = useQuery(activitySnapshotQueryOptions());
  const reading = useRef(false);
  const again = useRef(false);

  const refresh = useCallback(async () => {
    if (reading.current) {
      again.current = true;
      return;
    }
    reading.current = true;
    try {
      do {
        again.current = false;
        await client.refetchQueries({ queryKey: activityKeys.snapshot, exact: true });
      } while (again.current);
    } finally {
      reading.current = false;
    }
  }, [client]);

  useEffect(() => {
    const read = () => void refresh();
    const stopJobs = onJobStatus(read);
    const listening = ACTIVITY_REFRESH_EVENTS.map((name) => listen(name, read).catch(() => () => undefined));
    return () => {
      stopJobs();
      for (const attached of listening) void attached.then((unlisten) => unlisten());
    };
  }, [refresh]);

  return { snapshot: query.data, readAt: query.dataUpdatedAt, failed: query.isError, refresh };
}
