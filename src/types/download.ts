export type DownloadOperationState =
  | { type: 'DownloadStarted'; data: { total_size_bytes: number; files_to_download: number } }
  | { type: 'AlreadyDownloaded'; data: { verified_files: number; total_size_bytes: number } }
  | { type: 'NetworkError'; data: { error_message: string } }
  | { type: 'OperationFailed'; data: { error_message: string } };

export type DownloadModelResponse = import('../lib/bindings').DownloadModelResponse;

export function isDownloadSuccess(state: DownloadOperationState): boolean {
  return state.type === 'DownloadStarted' || state.type === 'AlreadyDownloaded';
}

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
