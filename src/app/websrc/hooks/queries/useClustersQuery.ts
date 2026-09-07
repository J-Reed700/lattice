import { useEffect, useState } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';

import VaultAPI from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { ClusterDto, ClusterProgressPayload, ClusterRunDto } from '@/types';

/**
 * The automatic groupings the vault falls into. Called clusters on the backend
 * (the table and commands predate the name); the UI says "themes".
 */
export const THEMES_QUERY_KEY = ['corpus', 'themes'] as const;

/** Below this, a flat list beats thinking about themes at all. */
export const THEMES_MIN_DOCUMENTS = 50;

export function useClustersQuery(enabled: boolean) {
  return useQuery<ClusterDto[]>({
    queryKey: THEMES_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.listClusters();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled,
    // Themes only change when the user asks for them.
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });
}

export function useRebuildThemes() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<ClusterRunDto> => {
      const result = await VaultAPI.clusterVaultRun();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    onSuccess: (run) => {
      void queryClient.invalidateQueries({ queryKey: THEMES_QUERY_KEY });
      toast.success(
        run.clusterCount === 0
          ? 'No themes found yet'
          : `${run.clusterCount} ${run.clusterCount === 1 ? 'theme' : 'themes'}`,
        run.noiseCount > 0
          ? { message: `${run.noiseCount.toLocaleString()} documents didn't fit one.` }
          : undefined,
      );
    },
    onError: (error: Error) => toast.error("Couldn't find themes", { message: error.message }),
  });
}

/**
 * The most recent step of an in-flight run, or null when nothing is running.
 *
 * `isRunning` matters: without it the last payload of the previous run survives,
 * and the next run opens on a stale "Saving…" until its first event lands.
 */
export function useRebuildProgress(isRunning = true): ClusterProgressPayload | null {
  const [progress, setProgress] = useState<ClusterProgressPayload | null>(null);

  useEffect(() => {
    if (!isRunning) setProgress(null);
  }, [isRunning]);

  useEffect(() => {
    const mounted = { current: true };
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        const handle = await listen<ClusterProgressPayload>('corpus-shape://progress', (event) => {
          setProgress(event.payload);
        });
        if (!mounted.current) {
          handle();
          return;
        }
        unlisten = handle;
      } catch {
        // Progress is decoration; a webview without the event bridge just
        // shows the indeterminate label.
      }
    })();

    return () => {
      mounted.current = false;
      unlisten?.();
    };
  }, []);

  return isRunning ? progress : null;
}
