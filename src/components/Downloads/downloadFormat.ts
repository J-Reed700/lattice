/**
 * Formatting helpers shared by the downloads UI.
 *
 * These deliberately live next to the components that use them: they encode
 * presentation decisions (how to render an unknown total, a stalled speed, a
 * missing ETA) that only matter to the drawer and its indicators.
 */

import { formatBytes } from '../../utils/formatters';

import type { DownloadState } from '../../types/downloads';

/** Bytes-per-second rendered as a transfer rate, e.g. `1.2 MB/s`. */
export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '—';
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** Seconds remaining rendered compactly, e.g. `3m 20s`. */
export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return '—';
  const total = Math.round(seconds);
  if (total < 60) return `${total}s`;
  const minutes = Math.floor(total / 60);
  if (minutes < 60) return `${minutes}m ${total % 60}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

/**
 * `downloaded / total` where total is known, otherwise just the downloaded
 * figure. The backend reports `total_bytes: null` for responses with no
 * `Content-Length`, which is common for HF redirects.
 */
export function formatProgressBytes(
  bytesDownloaded: number,
  totalBytes: number | null
): string {
  if (totalBytes === null || totalBytes <= 0) return formatBytes(bytesDownloaded);
  return `${formatBytes(bytesDownloaded)} / ${formatBytes(totalBytes)}`;
}

/**
 * A percentage clamped to 0–100, or `null` when the total size is unknown —
 * callers render an indeterminate bar in that case rather than a misleading 0%.
 */
export function normalizePercentage(
  percentage: number | null,
  bytesDownloaded: number,
  totalBytes: number | null
): number | null {
  if (percentage !== null && Number.isFinite(percentage)) {
    return Math.min(100, Math.max(0, percentage));
  }
  if (totalBytes !== null && totalBytes > 0) {
    return Math.min(100, Math.max(0, (bytesDownloaded / totalBytes) * 100));
  }
  return null;
}

/** Filename portion of a destination path, used as the display label. */
export function basename(destination: string): string {
  const parts = destination.split(/[\\/]/);
  return parts[parts.length - 1] || destination;
}

export const STATE_LABELS: Record<DownloadState, string> = {
  Pending: 'Queued',
  Downloading: 'Downloading',
  Paused: 'Paused',
  Completed: 'Completed',
  Failed: 'Failed',
  Cancelled: 'Cancelled',
};

/** Tailwind text colour per state, using the app's CSS custom properties. */
export const STATE_TEXT_CLASS: Record<DownloadState, string> = {
  Pending: 'text-[hsl(var(--text-muted))]',
  Downloading: 'text-[hsl(var(--accent))]',
  Paused: 'text-[hsl(var(--text-muted))]',
  Completed: 'text-[hsl(var(--success,var(--accent)))]',
  Failed: 'text-[hsl(var(--danger,0_70%_50%))]',
  Cancelled: 'text-[hsl(var(--text-muted))]',
};

/** Fill colour for the progress bar per state. */
export const STATE_BAR_CLASS: Record<DownloadState, string> = {
  Pending: 'bg-[hsl(var(--text-muted))]',
  Downloading: 'bg-[hsl(var(--accent))]',
  Paused: 'bg-[hsl(var(--text-muted))]',
  Completed: 'bg-[hsl(var(--success,var(--accent)))]',
  Failed: 'bg-[hsl(var(--danger,0_70%_50%))]',
  Cancelled: 'bg-[hsl(var(--text-muted))]',
};

/** States that represent work still in flight (or resumable). */
export function isActive(state: DownloadState): boolean {
  return state === 'Downloading' || state === 'Pending' || state === 'Paused';
}
