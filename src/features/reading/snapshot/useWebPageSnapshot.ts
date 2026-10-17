/**
 * The frozen copy of one cited page, kept current by the backend's events.
 *
 * No polling: a capture reports every status flip and progress step through
 * `web-page-snapshot:changed`, and the cache is updated from that.
 */

import { useCallback, useEffect, useMemo } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { EventSchemas, listenValidated, TauriEventNames } from '@/types/events';

import { captureWebPageSnapshot, getWebPageSnapshot, type WebPageSnapshotDto } from './api';
import { normalizeSnapshotUrl } from './snapshotFrameUrl';
import { type WebPageMode, useWebPageSnapshotStore } from './webPageSnapshotStore';

export const webPageSnapshotQueryKey = (conversationId: string, url: string) =>
  ['web-page-snapshot', conversationId, url] as const;

/** Every snapshot captured in a conversation. */
export const webPageSnapshotListQueryKey = (conversationId: string) =>
  ['web-page-snapshots', conversationId] as const;

export interface UseWebPageSnapshot {
  snapshot: WebPageSnapshotDto | null;
  /** True until the first answer about this page arrives. */
  isLoading: boolean;
  isCapturing: boolean;
  capture: () => void;
  retry: () => void;
}

export function useWebPageSnapshot(conversationId: string | null, url: string | null): UseWebPageSnapshot {
  const queryClient = useQueryClient();
  const enabled = Boolean(url);
  const queryKey = useMemo(
    () => webPageSnapshotQueryKey(conversationId ?? '', url ?? ''),
    [conversationId, url],
  );

  const query = useQuery({
    queryKey,
    queryFn: () => getWebPageSnapshot(conversationId ?? '', url ?? ''),
    enabled,
    // A missing command or a failed lookup means "show Reader", not "wait".
    retry: false,
  });

  useEffect(() => {
    if (!url) return;
    const target = normalizeSnapshotUrl(url);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenValidated(
      TauriEventNames.WebPageSnapshot.Changed,
      EventSchemas.WebPageSnapshot.Changed,
      ({ payload }) => {
        const conversationMatches = !conversationId || payload.conversationId === conversationId;
        void queryClient.invalidateQueries({ queryKey: webPageSnapshotListQueryKey(payload.conversationId) });
        const { snapshot } = payload;
        const matches =
          payload.url === url ||
          snapshot.requestedUrl === url ||
          snapshot.finalUrl === url ||
          (target !== null &&
            (snapshot.normalizedUrl.toLowerCase() === target.toLowerCase() ||
              normalizeSnapshotUrl(snapshot.requestedUrl)?.toLowerCase() === target.toLowerCase() ||
              (snapshot.finalUrl !== null &&
                normalizeSnapshotUrl(snapshot.finalUrl)?.toLowerCase() === target.toLowerCase())));
        if (conversationMatches && matches) queryClient.setQueryData(queryKey, snapshot);
      },
    ).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [conversationId, url, queryClient, queryKey]);

  const mutation = useMutation({
    mutationFn: () => captureWebPageSnapshot(conversationId ?? '', url ?? ''),
    onSuccess: (snapshot) => {
      void queryClient.invalidateQueries({ queryKey: webPageSnapshotListQueryKey(conversationId ?? '') });
      queryClient.setQueryData<WebPageSnapshotDto | null>(queryKey, (current) => {
        if (
          current &&
          current.id === snapshot.id &&
          current.status === 'ready' &&
          snapshot.status === 'pending'
        ) {
          return current;
        }
        return snapshot;
      });
    },
  });
  const { mutate } = mutation;
  const capture = useCallback(() => {
    if (enabled) mutate();
  }, [enabled, mutate]);

  return {
    snapshot: query.data ?? null,
    isLoading: enabled && query.isPending,
    isCapturing: mutation.isPending,
    capture,
    retry: capture,
  };
}

/**
 * Reader or Page for this cited page: the person's pick this session if they
 * made one, else Page when a ready frozen copy exists.
 */
export function useWebPageMode(conversationId: string | null, url: string | null) {
  const { snapshot, isLoading } = useWebPageSnapshot(conversationId, url);
  const chosen = useWebPageSnapshotStore((state) => state.mode);
  const setMode = useWebPageSnapshotStore((state) => state.setMode);
  const mode: WebPageMode = chosen ?? (snapshot?.status === 'ready' ? 'page' : 'reader');
  return { mode, setMode, snapshot, isResolving: chosen === null && isLoading };
}
