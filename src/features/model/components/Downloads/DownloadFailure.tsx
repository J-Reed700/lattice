import type { AggregatedDownload, DownloadStatus } from '@/types/downloads';

/** Why the last download failed, for a settings row that offers it again. Nothing when it did not fail. */
export function DownloadFailure({ download }: { download: DownloadStatus | AggregatedDownload | null | undefined }) {
  if (download?.state !== 'Failed') return null;
  return (
    <p role="status" className="text-xs text-danger-fg">
      {download.error_message ? `Couldn't download: ${download.error_message}` : 'The last download failed.'}
    </p>
  );
}
