/**
 * TypeScript types for download operation states
 *
 * Mirrors the Rust DownloadOperationState enum from domain/modules/download.rs
 */

export type DownloadOperationState =
  | { type: 'DownloadStarted'; data: { total_size_bytes: number; files_to_download: number } }
  | { type: 'AlreadyDownloaded'; data: { verified_files: number; total_size_bytes: number } }
  | { type: 'NetworkError'; data: { error_message: string } }
  | { type: 'OperationFailed'; data: { error_message: string } };

export interface DownloadModelResponse {
  state: DownloadOperationState;
  path: string | null;
}

/**
 * Type guard to check if operation was successful
 */
export function isDownloadSuccess(state: DownloadOperationState): boolean {
  return state.type === 'DownloadStarted' || state.type === 'AlreadyDownloaded';
}

/**
 * Get display message for download operation state
 */
export function getDownloadStateMessage(state: DownloadOperationState): string {
  switch (state.type) {
    case 'DownloadStarted':
      return `Download started (${state.data.files_to_download} files)`;
    case 'AlreadyDownloaded':
      return `Model already downloaded (${state.data.verified_files} files verified)`;
    case 'NetworkError':
      return `Network error: ${state.data.error_message}`;
    case 'OperationFailed':
      return `Operation failed: ${state.data.error_message}`;
  }
}
