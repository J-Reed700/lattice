import { useState } from 'react';

import { useDownloadActions } from '@/features/model/hooks/useDownloads';
import { useDownloadStore } from '@/features/model/stores/downloadStore';
import { toast } from '@/stores/toastStore';
import type { AggregatedDownload, DownloadStatus } from '@/types/downloads';

const LINK_BUTTON_CLASS =
  'text-xs text-text-secondary underline transition-colors duration-fast hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50';

interface InlineDownloadStatusProps {
  download: DownloadStatus | AggregatedDownload;
  /** A finished download whose owner has not committed it yet. */
  finishing?: boolean;
  /** What the region is called to a screen reader, e.g. "Downloading the reranker". */
  label: string;
}

/**
 * A settings row's view of a download: progress, Resume when paused, and a
 * way into the Downloads drawer. Every row that offers a download uses it, so
 * they all look and behave the same.
 */
export function InlineDownloadStatus({ download, finishing = false, label }: InlineDownloadStatusProps) {
  const { resumeDownload } = useDownloadActions();
  const openDrawer = useDownloadStore((state) => state.openDrawer);
  const [resuming, setResuming] = useState(false);

  const percent = finishing ? 100 : Math.round(download.percentage ?? 0);
  const paused = !finishing && download.state === 'Paused';
  const text = finishing
    ? 'Finishing…'
    : download.state === 'Pending'
      ? 'Queued'
      : paused
        ? 'Paused'
        : `Downloading… ${percent}%`;

  const resume = async () => {
    const files = 'downloads' in download ? download.downloads : [download];
    setResuming(true);
    try {
      for (const file of files) {
        if (file.state === 'Paused') await resumeDownload(file.id);
      }
    } catch (error) {
      toast.error("Couldn't resume the download", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setResuming(false);
    }
  };

  return (
    <div className="flex w-full max-w-[240px] flex-col gap-1" role="status" aria-label={label}>
      <progress className="h-1.5 w-full accent-accent" max={100} value={percent} />
      <span className="flex items-center justify-between gap-3 text-xs tabular-nums text-text-muted">
        <span>{text}</span>
        <span className="flex items-center gap-3">
          {paused ? (
            <button type="button" className={LINK_BUTTON_CLASS} disabled={resuming} onClick={() => void resume()}>
              Resume
            </button>
          ) : null}
          <button type="button" className={LINK_BUTTON_CLASS} onClick={openDrawer}>
            Show in Downloads
          </button>
        </span>
      </span>
    </div>
  );
}
