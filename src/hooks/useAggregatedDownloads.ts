import { useMemo } from 'react';

import type { DownloadStatus, AggregatedDownload, DownloadState } from '../types/downloads';

interface AggregatedDownloadsResult {
  aggregated: AggregatedDownload[];
  standalone: DownloadStatus[];
}

export function useAggregatedDownloads(downloads: DownloadStatus[]): AggregatedDownloadsResult {
  return useMemo(() => {
    const grouped = new Map<string, DownloadStatus[]>();
    const standalone: DownloadStatus[] = [];

    // Group downloads by model_id
    downloads.forEach((download) => {
      if (download.model_id) {
        const existing = grouped.get(download.model_id) || [];
        existing.push(download);
        grouped.set(download.model_id, existing);
      } else {
        standalone.push(download);
      }
    });

    const aggregated: AggregatedDownload[] = [];

    grouped.forEach((modelDownloads, modelId) => {
      // Only aggregate if there are multiple files for this model
      if (modelDownloads.length > 1) {
        const totalBytes = modelDownloads.reduce(
          (sum, d) => sum + (d.total_bytes || 0),
          0
        );
        const bytesDownloaded = modelDownloads.reduce(
          (sum, d) => sum + d.bytes_downloaded,
          0
        );
        const percentage = totalBytes > 0 ? (bytesDownloaded / totalBytes) * 100 : 0;
        const averageSpeed = modelDownloads.reduce(
          (sum, d) => sum + d.bytes_per_second,
          0
        ) / modelDownloads.length;

        // Determine aggregated state with correct precedence
        let state: DownloadState = 'Completed';
        let errorMessage: string | null = null;

        const hasPending = modelDownloads.some((d) => d.state === 'Pending');
        const hasDownloading = modelDownloads.some((d) => d.state === 'Downloading');
        const hasFailed = modelDownloads.some((d) => d.state === 'Failed');
        const hasPaused = modelDownloads.some((d) => d.state === 'Paused');
        const allCompleted = modelDownloads.every((d) => d.state === 'Completed');

        // State precedence: Downloading > Failed > Paused > Pending > Completed
        // Active states (Downloading, Failed) take priority over passive states (Pending, Paused)
        if (hasDownloading) {
          state = 'Downloading';
        } else if (hasFailed) {
          state = 'Failed';
          const failedDownload = modelDownloads.find((d) => d.state === 'Failed');
          errorMessage = failedDownload?.error_message || 'One or more files failed';
        } else if (hasPaused) {
          state = 'Paused';
        } else if (hasPending) {
          state = 'Pending';
        } else if (!allCompleted) {
          state = 'Pending';
        }

        aggregated.push({
          model_id: modelId,
          model_name: modelDownloads[0].model_name || modelId,
          downloads: modelDownloads,
          total_bytes: totalBytes,
          bytes_downloaded: bytesDownloaded,
          percentage,
          average_speed: averageSpeed,
          state,
          error_message: errorMessage,
        });
      } else {
        // Single file for this model - treat as standalone
        standalone.push(...modelDownloads);
      }
    });

    return { aggregated, standalone };
  }, [downloads]);
}
