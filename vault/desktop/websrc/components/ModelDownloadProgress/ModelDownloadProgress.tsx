import { useState, useEffect } from 'react';

import { useDownloadState } from '../../hooks/useDownloadState';

interface ModelDownloadProgressProps {
  onError?: (error: string) => void;
}

export function ModelDownloadProgress({
  onError
}: ModelDownloadProgressProps) {
  const [manuallyDismissed, setManuallyDismissed] = useState(false);

  const { activeDownloads } = useDownloadState();

  const hasActiveDownloads = activeDownloads.length > 0;

  // Calculate aggregated progress across all active downloads
  const totalBytes = activeDownloads.reduce((sum, d) => sum + (d.total_bytes || 0), 0);
  const downloadedBytes = activeDownloads.reduce((sum, d) => sum + d.bytes_downloaded, 0);
  const averageSpeed = activeDownloads.reduce((sum, d) => sum + d.bytes_per_second, 0);
  // Avoid division by zero
  const percentage = totalBytes > 0 ? (downloadedBytes / totalBytes) * 100 : 0;

  // ETA calculation: remaining bytes / average speed
  const remainingBytes = totalBytes - downloadedBytes;
  const etaSeconds = averageSpeed > 0 ? remainingBytes / averageSpeed : null;

  // Find any failed download
  const failedDownload = activeDownloads.find(d => d.state === 'Failed');
  const error = failedDownload?.error_message || null;

  // Auto-show modal when downloads start (rising edge detection with useEffect)
  useEffect(() => {
    if (hasActiveDownloads) {
      setManuallyDismissed(false);
    }
  }, [hasActiveDownloads]); // Only runs when active status changes

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))  } ${  sizes[i]}`;
  };

  const formatTime = (seconds: number): string => {
    if (!isFinite(seconds)) return 'Calculating...';
    if (seconds < 60) return `${Math.round(seconds)}s`;
    const minutes = Math.floor(seconds / 60);
    const secs = Math.round(seconds % 60);
    return `${minutes}m ${secs}s`;
  };

  // VISIBILITY LOGIC:
  // 1. If manually dismissed, hide.
  // 2. If no active downloads AND no error, hide.
  if (manuallyDismissed || (!hasActiveDownloads && !error)) {
    return null;
  }

  // ERROR STATE:
  // Only show full blocking error if NO downloads are active (complete failure)
  // Otherwise, show progress for remaining files even if one file failed
  if (error && !hasActiveDownloads) {
    return (
      <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
        <div className="bg-[var(--surface-elevated)] rounded-lg p-6 max-w-md w-full mx-4 shadow-xl">
          <div className="flex items-center mb-4">
            <div className="w-12 h-12 bg-[var(--error-light)] rounded-full flex items-center justify-center mr-4">
              <svg
                className="w-6 h-6 text-[var(--error)]"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </div>
            <div>
              <h2 className="text-xl font-semibold text-[var(--text-primary)]">Download Failed</h2>
            </div>
          </div>
          <div className="bg-[var(--error-light)] border border-[var(--error-light)] rounded p-3 mb-4">
            <p className="text-sm text-[var(--error)]">{error}</p>
          </div>
          <div className="flex justify-end space-x-3">
            <button
              onClick={() => {
                setManuallyDismissed(true);
                onError?.(error);
              }}
              className="px-4 py-2 bg-[var(--surface-elevated)] text-[var(--text-primary)] border border-[var(--border-color)] rounded hover:bg-[var(--bg-secondary)]"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Show progress for active downloads

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-[var(--surface-elevated)] rounded-lg p-6 max-w-md w-full mx-4 shadow-xl">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center">
            <div className="w-12 h-12 bg-[var(--accent-light)] rounded-full flex items-center justify-center mr-4 animate-pulse">
              <svg
                className="w-6 h-6 text-[var(--accent-primary)]"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                />
              </svg>
            </div>
            <div>
              <h2 className="text-xl font-semibold text-[var(--text-primary)]">Downloading Model</h2>
              <p className="text-sm text-[var(--text-secondary)]">
                {activeDownloads.length} file{activeDownloads.length !== 1 ? 's' : ''} in progress...
              </p>
            </div>
          </div>
          <button
            onClick={() => setManuallyDismissed(true)}
            className="text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors"
            aria-label="Close"
          >
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {hasActiveDownloads && (
          <>
            <div className="mb-2">
              <div className="flex justify-between text-sm text-[var(--text-secondary)] mb-1">
                <span className="font-medium">Overall Progress</span>
                <span className="font-semibold">{percentage.toFixed(1)}%</span>
              </div>
              <div className="w-full bg-[var(--bg-tertiary)] rounded-full h-2.5 overflow-hidden">
                <div
                  className="bg-[var(--accent-primary)] h-2.5 rounded-full transition-all duration-300 ease-out"
                  style={{ width: `${percentage}%` }}
                />
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4 mt-4">
              <div>
                <p className="text-xs text-[var(--text-tertiary)] mb-1">Downloaded</p>
                <p className="text-sm font-semibold text-[var(--text-primary)]">
                  {formatBytes(downloadedBytes)} / {formatBytes(totalBytes)}
                </p>
              </div>
              <div>
                <p className="text-xs text-[var(--text-tertiary)] mb-1">Speed</p>
                <p className="text-sm font-semibold text-[var(--text-primary)]">
                  {formatBytes(averageSpeed)}/s
                </p>
              </div>
              <div>
                <p className="text-xs text-[var(--text-tertiary)] mb-1">Time Remaining</p>
                <p className="text-sm font-semibold text-[var(--text-primary)]">
                  {etaSeconds !== null ? formatTime(etaSeconds) : 'Calculating...'}
                </p>
              </div>
              <div>
                <p className="text-xs text-[var(--text-tertiary)] mb-1">Status</p>
                <p className="text-sm font-semibold text-[var(--success)]">
                  Downloading...
                </p>
              </div>
            </div>

            {/* Show individual file progress if multiple files */}
            {activeDownloads.length > 1 && (
              <div className="mt-4 pt-4 border-t border-[var(--border-color)]">
                <p className="text-xs text-[var(--text-tertiary)] mb-2">Active Downloads:</p>
                <div className="space-y-2 max-h-32 overflow-y-auto">
                  {activeDownloads.map(download => (
                    <div key={download.id} className="text-xs">
                      <div className="flex justify-between text-[var(--text-secondary)]">
                        <span className="truncate">{download.url.split('/').pop()}</span>
                        <span>{download.percentage !== null ? `${download.percentage.toFixed(0)}%` : 'Pending'}</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}

        {!hasActiveDownloads && (
          <div className="flex items-center justify-center py-8">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-[var(--accent-primary)]" />
            <span className="ml-3 text-[var(--text-secondary)]">Initializing download...</span>
          </div>
        )}

        <div className="mt-6 pt-4 border-t border-[var(--border-color)]">
          <p className="text-xs text-[var(--text-tertiary)] text-center">
            Downloading embedding model from HuggingFace (~90MB)
            <br />
            This only happens once on first launch
          </p>
        </div>
      </div>
    </div>
  );
}
