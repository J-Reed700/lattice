import type { RerankerStatusDto } from '@/lib/bindings';


export function formatDownloadSize(bytes: number): string {
  const megabytes = bytes / 1_000_000;
  return megabytes >= 1000
    ? `${(megabytes / 1000).toFixed(1)} GB`
    : `${Math.round(megabytes)} MB`;
}

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
  if (!status.installed) {
    return status.enabled
      ? 'Switched on, but the reranker model is not downloaded, so results are not being reranked.'
      : 'Needs a one-time model download before it can run.';
  }
  return status.enabled
    ? 'A cross-encoder rescores the shortlist before the answer is written.'
    : 'Model ready. Switch on to rescore the shortlist before answering.';
}
