import type { DownloadState } from '@/types/downloads';
import { formatBytes } from '@/utils/formatters';

export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '—';
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return '—';
  const total = Math.round(seconds);
  if (total < 60) return `${total}s`;
  const minutes = Math.floor(total / 60);
  if (minutes < 60) return `${minutes}m ${total % 60}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function formatProgressBytes(
  bytesDownloaded: number,
  totalBytes: number | null
): string {
  if (totalBytes === null || totalBytes <= 0) return formatBytes(bytesDownloaded);
  return `${formatBytes(bytesDownloaded)} / ${formatBytes(totalBytes)}`;
}

export function normalizePercentage(
  percentage: number | null,
  bytesDownloaded: number,
  totalBytes: number | null
): number | null {
  if (percentage !== null && Number.isFinite(percentage)) {
    return Math.min(100, Math.max(0, percentage));
  }
  if (totalBytes !== null && Number.isFinite(totalBytes) && totalBytes > 0 && Number.isFinite(bytesDownloaded)) {
    return Math.min(100, Math.max(0, (bytesDownloaded / totalBytes) * 100));
  }
  return null;
}

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

export const STATE_TEXT_CLASS: Record<DownloadState, string> = {
  Pending: 'text-[hsl(var(--text-muted))]',
  Downloading: 'text-[hsl(var(--accent))]',
  Paused: 'text-[hsl(var(--text-muted))]',
  Completed: 'text-[hsl(var(--success,var(--accent)))]',
  Failed: 'text-[hsl(var(--danger,0_70%_50%))]',
  Cancelled: 'text-[hsl(var(--text-muted))]',
};

export const STATE_BAR_CLASS: Record<DownloadState, string> = {
  Pending: 'bg-[hsl(var(--text-muted))]',
  Downloading: 'bg-[hsl(var(--accent))]',
  Paused: 'bg-[hsl(var(--text-muted))]',
  Completed: 'bg-[hsl(var(--success,var(--accent)))]',
  Failed: 'bg-[hsl(var(--danger,0_70%_50%))]',
  Cancelled: 'bg-[hsl(var(--text-muted))]',
};

export function isActive(state: DownloadState): boolean {
  return state === 'Downloading' || state === 'Pending' || state === 'Paused';
}
