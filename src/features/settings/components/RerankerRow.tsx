import { useIsMutating } from '@tanstack/react-query';

import { SettingsRow } from '@/components/ui';
import { InlineDownloadStatus } from '@/features/model/components/Downloads/InlineDownloadStatus';
import { InstalledDownload } from '@/features/model/components/Downloads/InstalledDownload';
import { useDownloadPhase } from '@/features/model/hooks/useDownloadPhase';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { latestDownloadFor } from '@/features/model/utils/downloadPhase';

import { describeReranker, formatDownloadSize } from './rerankerStatus';
import {
  DOWNLOAD_RERANKER_MUTATION_KEY,
  useDownloadRerankerMutation,
  useRemoveRerankerMutation,
  useRerankerStatusQuery,
} from '../hooks/useRerankerQuery';

/**
 * The reranker's download, driven by the shared download list: it matches the
 * download by the model's path, so the row reads the same after navigating
 * away or restarting as it did when the download began.
 */
export function RerankerRow() {
  const { data: status, isFetchedAfterMount, refetch } = useRerankerStatusQuery();
  const { mutate: startDownload } = useDownloadRerankerMutation();
  const { mutateAsync: removeReranker } = useRemoveRerankerMutation();
  // The mutation cache outlives this row, so "Starting…" survives a remount.
  const starting = useIsMutating({ mutationKey: DOWNLOAD_RERANKER_MUTATION_KEY }) > 0;
  const { downloads, downloadsSettled } = useDownloadState();

  const download = latestDownloadFor(downloads, status?.path);
  const phase = useDownloadPhase({ ready: status?.installed ?? false, download, poll: refetch });

  return (
    <SettingsRow label="Reranker" hint={describeReranker(status, phase, download)}>
      {(phase === 'live' || phase === 'finishing') && download ? (
        <InlineDownloadStatus download={download} finishing={phase === 'finishing'} label="Downloading the reranker" />
      ) : status && phase === 'ready' ? (
        <InstalledDownload
          name="the reranker"
          consequence="Search results keep their first-stage order until you download it again."
          size={formatDownloadSize(status.downloadBytes)}
          onRemove={() => removeReranker()}
        />
      ) : status && phase === 'idle' ? (
        <button
          type="button"
          onClick={() => startDownload()}
          disabled={starting || !isFetchedAfterMount || !downloadsSettled}
          className="h-8 shrink-0 rounded-sm border border-border-default px-2.5 text-sm text-text-primary transition-colors duration-fast hover:border-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          {starting ? 'Starting…' : `Download (${formatDownloadSize(status.downloadBytes)})`}
        </button>
      ) : null}
    </SettingsRow>
  );
}
