import type { AggregatedDownload, DownloadState, DownloadStatus } from '@/types/downloads';

/** How long a finished download may wait for its owner to commit before the row gives up on it. */
export const FINISHING_WINDOW_MS = 60_000;

export type DownloadPhase = 'ready' | 'live' | 'finishing' | 'idle';

const LIVE_STATES: ReadonlySet<DownloadState> = new Set(['Pending', 'Downloading', 'Paused']);

export const isLiveState = (state: DownloadState): boolean => LIVE_STATES.has(state);

const normalizePath = (path: string): string => path.replace(/\\/g, '/');

const createdAt = (download: DownloadStatus): number => {
  const time = Date.parse(download.created_at);
  return Number.isNaN(time) ? 0 : time;
};

/**
 * The download that describes a file, by its final path. A live row wins, so a
 * stale Cancelled leftover never hides a running download; with none live, the
 * newest row wins, so a Cancelled latest reads as idle.
 */
export function latestDownloadFor(
  downloads: readonly DownloadStatus[],
  path: string | null | undefined
): DownloadStatus | null {
  if (!path) return null;
  const target = normalizePath(path);
  let latest: DownloadStatus | null = null;
  let latestLive: DownloadStatus | null = null;
  for (const download of downloads) {
    if (normalizePath(download.destination) !== target) continue;
    if (!latest || createdAt(download) >= createdAt(latest)) latest = download;
    if (isLiveState(download.state) && (!latestLive || createdAt(download) >= createdAt(latestLive))) {
      latestLive = download;
    }
  }
  return latestLive ?? latest;
}

const isAggregated = (download: DownloadStatus | AggregatedDownload): download is AggregatedDownload =>
  'downloads' in download;

/** When the download finished, in epoch ms; for a group, its last file. */
export function completedAtMs(download: DownloadStatus | AggregatedDownload | null | undefined): number | null {
  if (!download) return null;
  const stamps = (isAggregated(download) ? download.downloads : [download])
    .map((file) => (file.completed_at ? Date.parse(file.completed_at) : Number.NaN))
    .filter((time) => !Number.isNaN(time));
  return stamps.length > 0 ? Math.max(...stamps) : null;
}

interface PhaseInput {
  /** The row's own status says the file or model is usable. */
  ready: boolean;
  download: DownloadStatus | AggregatedDownload | null | undefined;
  now: number;
}

/**
 * Where a download row stands. A model counts as ready only after its owner
 * commits it, which is after the download completes, so a finished download
 * is "finishing" for a minute rather than offering a second download.
 */
export function downloadPhase({ ready, download, now }: PhaseInput): DownloadPhase {
  if (ready) return 'ready';
  if (!download) return 'idle';
  if (isLiveState(download.state)) return 'live';
  if (download.state === 'Completed') {
    const completedAt = completedAtMs(download);
    if (completedAt !== null && now - completedAt < FINISHING_WINDOW_MS) return 'finishing';
  }
  return 'idle';
}
