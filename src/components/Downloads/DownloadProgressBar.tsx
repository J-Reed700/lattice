import { STATE_BAR_CLASS } from './downloadFormat';

import type { DownloadState } from '../../types/downloads';

interface DownloadProgressBarProps {
  /** 0–100, or `null` when the total size is unknown (renders indeterminate). */
  percentage: number | null;
  state: DownloadState;
  label: string;
}

/**
 * A thin progress track. When `percentage` is null the backend didn't report a
 * `Content-Length`, so we show an indeterminate shimmer rather than a bar stuck
 * at 0% — the download is progressing, we just can't say how far along it is.
 */
export function DownloadProgressBar({ percentage, state, label }: DownloadProgressBarProps) {
  const indeterminate = percentage === null;
  const barClass = STATE_BAR_CLASS[state];

  return (
    <div
      className="h-1 w-full overflow-hidden rounded-full bg-[hsl(var(--surface-raised))]"
      role="progressbar"
      aria-label={label}
      aria-valuemin={indeterminate ? undefined : 0}
      aria-valuemax={indeterminate ? undefined : 100}
      aria-valuenow={indeterminate ? undefined : Math.round(percentage)}
      aria-valuetext={indeterminate ? 'Progress unknown' : `${Math.round(percentage)}%`}
    >
      {indeterminate ? (
        <div className={`h-full w-1/3 animate-pulse rounded-full ${barClass} opacity-60`} />
      ) : (
        <div
          className={`h-full rounded-full transition-[width] duration-300 ease-out ${barClass}`}
          style={{ width: `${percentage}%` }}
        />
      )}
    </div>
  );
}
