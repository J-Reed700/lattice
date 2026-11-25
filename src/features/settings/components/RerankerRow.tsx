import { useIsMutating } from '@tanstack/react-query';

import { Button, SettingsRow } from '@/components/ui';
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
  const { data: status, isPending, isFetchedAfterMount, refetch } = useRerankerStatusQuery();
  const { mutate: startDownload } = useDownloadRerankerMutation();
  const { mutateAsync: removeReranker } = useRemoveRerankerMutation();
  // The mutation cache outlives this row, so "Starting…" survives a remount.
  const starting = useIsMutating({ mutationKey: DOWNLOAD_RERANKER_MUTATION_KEY }) > 0;
  const { downloads, downloadsSettled } = useDownloadState();

  const download = latestDownloadFor(downloads, status?.path);
  const phase = useDownloadPhase({ ready: status?.installed ?? false, download, poll: refetch });

  const hint =
    describeReranker(status, phase, download) ??
    (isPending
      ? 'Checking whether the local cross-encoder model is installed.'
      : 'Not configured — results keep their first-stage search order.');

  return (
    <SettingsRow label="Reranker" hint={hint}>
      {(phase === 'live' || phase === 'finishing') && download ? (
        <InlineDownloadStatus
          download={download}
          finishing={phase === 'finishing'}
          label="Downloading the reranker"
        />
      ) : status && phase === 'ready' ? (
        <InstalledDownload
          name="the reranker"
          consequence="Search results keep their first-stage order until you download it again."
          size={formatDownloadSize(status.downloadBytes)}
          onRemove={() => removeReranker()}
        />
      ) : status && phase === 'idle' ? (
        <Button
          type="button"
          variant="outline"
          onClick={() => startDownload()}
          disabled={starting || !isFetchedAfterMount || !downloadsSettled}
        >
          {starting ? 'Starting…' : `Download (${formatDownloadSize(status.downloadBytes)})`}
        </Button>
      ) : isPending ? (
        <span className="text-xs text-text-muted">Checking…</span>
      ) : (
        <div className="flex items-center gap-2.5">
          <span className="text-xs text-text-muted">Not configured</span>
          <Button type="button" variant="outline" size="sm" onClick={() => void refetch()}>
            Retry
          </Button>
        </div>
      )}
    </SettingsRow>
  );
}
