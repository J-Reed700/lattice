import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';
import type { StartDownloadRequest, DownloadStatus } from '@/types/downloads';

function normalizeDownloadStatus(
  raw: Wire.DownloadStatusResponse,
): DownloadStatus {
  switch (raw.state) {
    case 'Pending':
    case 'Downloading':
    case 'Paused':
    case 'Completed':
    case 'Failed':
    case 'Cancelled':
      return { ...raw, state: raw.state };
    default:
      throw new Error(`Unknown download state: ${raw.state}`);
  }
}
export const downloadApi = {
  /**
   * Starts downloading a model from a URL.
   * Supports resumable downloads with checksum verification.
   *
   * @param request - Download configuration including URL, destination, and metadata
   * @returns Download ID for tracking progress
   *
   * @example
   * const result = await VaultAPI.startModelDownload({
   *   url: 'https://huggingface.co/model/file.gguf',
   *   destination: '/path/to/models/model.gguf',
   *   model_name: 'My Model',
   *   model_id: 'my-model-id'
   * });
   */
  startModelDownload: async (
    request: StartDownloadRequest,
  ): Promise<ApiResult<string>> =>
    apiCall<string>('start_model_download', { request }),

  /**
   * Pauses an ongoing download.
   * Download can be resumed later from where it stopped.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  pauseDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('pause_download', { id: downloadId }),

  /**
   * Resumes a paused download.
   * Continues from the last downloaded byte.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  resumeDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('resume_download', { id: downloadId }),

  /**
   * Cancels an ongoing or paused download.
   * Deletes partial download file.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  cancelDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('cancel_download', { id: downloadId }),

  /**
   * Retries a failed download.
   * Attempts to restart a download that encountered an error.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  retryDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('retry_download', { id: downloadId }),

  /**
   * Removes a download from the download list.
   * Deletes download record and any partial files.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  removeDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_download', { id: downloadId }),

  /**
   * Clears all completed downloads from the download list.
   * Removes download records for successful downloads.
   *
   * @returns Number of downloads cleared
   */
  clearCompletedDownloads: async (): Promise<ApiResult<number>> =>
    apiCall<number>('clear_completed_downloads'),

  /**
   * Gets the current status and progress of a download.
   * Returns null if download ID doesn't exist.
   *
   * @param downloadId - Unique download identifier
   * @returns Download status with progress information or null
   */
  getDownloadStatus: async (
    downloadId: string,
  ): Promise<ApiResult<DownloadStatus | null>> =>
    apiCall<Wire.DownloadStatusResponse | null>('get_download_status', {
      id: downloadId,
    }).then((result) =>
      result.ok
        ? {
            ok: true,
            data:
              result.data === null
                ? null
                : normalizeDownloadStatus(result.data),
          }
        : result,
    ),

  /**
   * Lists all downloads (active, paused, completed, failed).
   * Returns comprehensive list for download management UI.
   *
   * @returns Array of all download statuses
   */
  listDownloads: async (): Promise<ApiResult<DownloadStatus[]>> =>
    apiCall<Wire.DownloadStatusResponse[]>('list_downloads').then((result) =>
      result.ok
        ? { ok: true, data: result.data.map(normalizeDownloadStatus) }
        : result,
    ),
};
