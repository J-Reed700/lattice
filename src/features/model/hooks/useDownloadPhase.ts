import { useEffect, useRef, useState } from 'react';

import type { AggregatedDownload, DownloadStatus } from '@/types/downloads';

import { completedAtMs, downloadPhase, FINISHING_WINDOW_MS, type DownloadPhase } from '../utils/downloadPhase';

/** How often a finishing row re-reads its status while it waits for the owner to commit. */
export const FINISHING_POLL_MS = 1000;

interface Options {
  ready: boolean;
  download: DownloadStatus | AggregatedDownload | null | undefined;
  /** Re-reads the row's status; called every second while the phase is finishing. */
  poll?: () => unknown;
}

/**
 * `downloadPhase` for a mounted row: re-renders when the finishing window
 * ends, so the row falls back to idle without needing a fetch, and polls the
 * row's status while it is finishing.
 */
export function useDownloadPhase({ ready, download, poll }: Options): DownloadPhase {
  const [now, setNow] = useState(() => Date.now());
  const phase = downloadPhase({ ready, download, now });
  const completedAt = completedAtMs(download);
  const pollRef = useRef(poll);
  useEffect(() => {
    pollRef.current = poll;
  });

  useEffect(() => {
    if (completedAt === null) return;
    const remaining = completedAt + FINISHING_WINDOW_MS - Date.now();
    if (remaining <= 0) return;
    const timer = setTimeout(() => setNow(Date.now()), remaining);
    return () => clearTimeout(timer);
  }, [completedAt]);

  const finishing = phase === 'finishing';
  useEffect(() => {
    if (!finishing) return;
    const timer = setInterval(() => void pollRef.current?.(), FINISHING_POLL_MS);
    return () => clearInterval(timer);
  }, [finishing]);

  return phase;
}
