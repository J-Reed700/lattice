import type { DownloadPhase } from '@/features/model/utils/downloadPhase';
import type { RerankerStatusDto } from '@/lib/bindings';
import type { AggregatedDownload, DownloadStatus } from '@/types/downloads';


export function formatDownloadSize(bytes: number): string {
  const megabytes = bytes / 1_000_000;
  return megabytes >= 1000
    ? `${(megabytes / 1000).toFixed(1)} GB`
    : `${Math.round(megabytes)} MB`;
}

/**
 * Say what reranking is doing. Search always reranks once the model is on
 * disk; until it is downloaded, results keep their first-stage order.
 */
export function describeReranker(
  status: RerankerStatusDto | undefined,
  phase: DownloadPhase,
  download?: DownloadStatus | AggregatedDownload | null
): string | undefined {
  if (!status) {
    return undefined;
  }
  if (phase === 'ready') {
    return 'A cross-encoder rescores the shortlist before the answer is written.';
  }
  if (phase === 'live' || phase === 'finishing') {
    return 'Reranking turns on when the download finishes.';
  }
  if (download?.state === 'Failed') {
    return download.error_message
      ? `Couldn't download the reranker: ${download.error_message}`
      : "Couldn't download the reranker.";
  }
  return 'Not downloaded yet, so results are not being reranked. A one-time download turns it on.';
}
