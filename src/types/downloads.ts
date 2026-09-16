export type DownloadState = 'Pending' | 'Downloading' | 'Paused' | 'Completed' | 'Failed' | 'Cancelled';

export interface ChecksumRequest {
  algorithm: 'sha256' | 'md5';
  value: string;
}

export interface StartDownloadRequest {
  url: string;
  destination: string;
  checksum?: ChecksumRequest;
  auth_token?: string;
  model_name?: string;
  model_id?: string;
}

export type DownloadStatus = Omit<import('../lib/bindings').DownloadStatusResponse, 'state'> & { state: DownloadState };

export interface DownloadEvent {
  type: 'started' | 'progress' | 'paused' | 'resumed' | 'completed' | 'failed' | 'cancelled';
  id: string;
  // Started event fields
  url?: string;
  destination?: string;
  model_name?: string;
  model_id?: string;
  // Progress event fields
  bytes_downloaded?: number;
  total_bytes?: number | null;
  bytes_per_second?: number;
  percentage?: number | null;
  eta_seconds?: number | null;
  // Failed event fields
  error?: string;
  // Completed event fields
  total_bytes_completed?: number;
  elapsed_seconds?: number;
}

export interface DownloadProgressInfo {
  percentage: number;
  bytesDownloaded: number;
  totalBytes: number | null;
  speed: number;
  etaSeconds: number | null;
  state: DownloadState;
}

export interface AggregatedDownload {
  model_id: string;
  model_name: string;
  downloads: DownloadStatus[];
  total_bytes: number;
  bytes_downloaded: number;
  percentage: number;
  average_speed: number;
  state: DownloadState;
  error_message: string | null;
}
