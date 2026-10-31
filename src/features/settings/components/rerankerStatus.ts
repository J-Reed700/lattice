import type { RerankerStatusDto } from '@/lib/bindings';


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
  downloadError?: Error | null
): string | undefined {
  if (downloadError) {
    return `Could not download the reranker: ${downloadError.message}`;
  }
  if (!status) {
    return undefined;
  }
  return status.installed
    ? 'A cross-encoder rescores the shortlist before the answer is written.'
    : 'Not downloaded yet, so results are not being reranked. A one-time download turns it on.';
}
