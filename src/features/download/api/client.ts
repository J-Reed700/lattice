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
  startModelDownload: async (
    request: StartDownloadRequest,
  ): Promise<ApiResult<string>> =>
    apiCall<string>('start_model_download', { request }),

  pauseDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('pause_download', { id: downloadId }),

  resumeDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('resume_download', { id: downloadId }),

  startVisionProjectorDownload: async (id: string): Promise<ApiResult<Wire.VisionProjectorDownloadDto>> =>
    apiCall<Wire.VisionProjectorDownloadDto>('start_vision_projector_download', { id }),

  cancelDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('cancel_download', { id: downloadId }),

  retryDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('retry_download', { id: downloadId }),

  removeDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_download', { id: downloadId }),

  clearCompletedDownloads: async (): Promise<ApiResult<number>> =>
    apiCall<number>('clear_completed_downloads'),

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

  listDownloads: async (): Promise<ApiResult<DownloadStatus[]>> =>
    apiCall<Wire.DownloadStatusResponse[]>('list_downloads').then((result) =>
      result.ok
        ? { ok: true, data: result.data.map(normalizeDownloadStatus) }
        : result,
    ),
};
