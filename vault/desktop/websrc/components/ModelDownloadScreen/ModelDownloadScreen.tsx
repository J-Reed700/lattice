/**
 * ModelDownloadScreen - Enhanced model download with progress tracking
 *
 * Purpose: Transform the anxiety of "what's happening?" into confidence through
 * transparent, informative progress tracking. Users need to see and understand
 * what's being downloaded, why, and how long it will take.
 *
 * Features:
 * - Real-time progress with percentage, speed, and ETA
 * - File-by-file breakdown (model.onnx, tokenizer.json, etc.)
 * - Error recovery with retry mechanism
 * - Background mode option (for re-downloads)
 * - Clear status messages at each stage
 *
 * States: initializing, downloading, verifying, complete, error
 * Accessibility: Progress announcements, keyboard navigation, screen reader support
 */

import { useEffect, useState } from 'react';

import { Download, CheckCircle2, AlertCircle, RefreshCw, Minimize2 } from 'lucide-react';

import { TauriEventNames, EventSchemas, listenValidated } from '../../types/events';

interface DownloadProgress {
  bytes_downloaded: number;
  total_bytes: number;
  percentage: number;
  speed_mbps: number;
  eta_seconds: number;
  current_file: string;
}

interface ModelDownloadScreenProps {
  onComplete?: () => void;
  onError?: (error: string) => void;
  embedded?: boolean; // When true, renders without modal overlay (for onboarding)
  allowBackground?: boolean; // Allow user to continue in background
}

type DownloadStatus = 'initializing' | 'downloading' | 'verifying' | 'complete' | 'error';

export function ModelDownloadScreen({
  onComplete,
  onError,
  embedded = false,
  allowBackground = false,
}: ModelDownloadScreenProps) {
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [status, setStatus] = useState<DownloadStatus>('initializing');
  const [error, setError] = useState<string | null>(null);
  const [isBackgrounded, setIsBackgrounded] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      try {
        // Listen for all download events with runtime validation
        unlisten = await listenValidated(
          TauriEventNames.Downloads.Event,
          EventSchemas.Downloads.StateSnapshot,
          (event) => {
            const payload = event.payload;

            // Handle different download states (discriminated by status field)
            switch (payload.status) {
              case 'downloading': {
                // Extract progress data from either Single or Batch snapshot
                const bytesDownloaded = payload.kind === 'single'
                  ? payload.bytesDownloaded
                  : payload.aggregateBytesDownloaded;
                const totalBytes = payload.kind === 'single'
                  ? (payload.totalBytes || 0)
                  : payload.aggregateTotalBytes;
                const bytesPerSecond = payload.kind === 'single'
                  ? payload.bytesPerSecond
                  : payload.aggregateBytesPerSecond;
                const percentage = payload.kind === 'single'
                  ? (payload.percentage || 0)
                  : payload.aggregatePercentage;
                const etaSeconds = payload.kind === 'single'
                  ? (payload.etaSeconds || 0)
                  : (payload.aggregateEtaSeconds || 0);
                const currentFile = payload.kind === 'single'
                  ? payload.filename
                  : payload.groupName;

                setProgress({
                  bytes_downloaded: bytesDownloaded,
                  total_bytes: totalBytes,
                  percentage,
                  speed_mbps: bytesPerSecond / 1_048_576, // Convert to MB/s
                  eta_seconds: etaSeconds,
                  current_file: currentFile,
                });
                setStatus('downloading');
                break;
              }

              case 'completed': {
                setStatus('complete');
                setTimeout(() => {
                  onComplete?.();
                }, 1500);
                break;
              }

              case 'error': {
                const errorMessage = 'Download failed'; // StateSnapshot doesn't include error message
                setError(errorMessage);
                setStatus('error');
                onError?.(errorMessage);
                break;
              }

              case 'cancelled': {
                setError('Download cancelled by user');
                setStatus('error');
                onError?.('Download cancelled');
                break;
              }

              // Other statuses (pending, paused, resumed) don't change UI state significantly
              default:
                break;
            }
          },
          (error) => {
            console.error('[ModelDownloadScreen] Event validation error:', error.format());
          }
        );
      } catch (err) {
        console.error('[ModelDownloadScreen] Failed to setup event listener:', err);
      }
    };

    setupListener();

    return () => {
      unlisten?.();
    };
  }, [onComplete, onError]);

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${Math.round((bytes / Math.pow(k, i)) * 100) / 100  } ${  sizes[i]}`;
  };

  const formatTime = (seconds: number): string => {
    if (seconds < 60) return `${Math.round(seconds)}s`;
    const minutes = Math.floor(seconds / 60);
    const secs = Math.round(seconds % 60);
    return `${minutes}m ${secs}s`;
  };

  const handleRetry = () => {
    setError(null);
    setStatus('initializing');
    window.location.reload();
  };

  const handleBackground = () => {
    setIsBackgrounded(true);
  };

  const getStatusMessage = (): string => {
    switch (status) {
      case 'initializing':
        return 'Preparing download...';
      case 'downloading':
        return `Downloading ${progress?.current_file || 'model files'}...`;
      case 'verifying':
        return 'Verifying integrity...';
      case 'complete':
        return 'Download complete!';
      case 'error':
        return 'Download failed';
      default:
        return 'Initializing...';
    }
  };

  const getStatusIcon = () => {
    switch (status) {
      case 'downloading':
        return <Download className="w-6 h-6 animate-pulse" />;
      case 'complete':
        return <CheckCircle2 className="w-6 h-6" />;
      case 'error':
        return <AlertCircle className="w-6 h-6" />;
      default:
        return <Download className="w-6 h-6" />;
    }
  };

  const getStatusColor = () => {
    switch (status) {
      case 'downloading':
        return 'text-[var(--accent-primary)] bg-[var(--accent-light)]';
      case 'complete':
        return 'text-[var(--success)] bg-[var(--success-light)]';
      case 'error':
        return 'text-[var(--error)] bg-[var(--error-light)]';
      default:
        return 'text-[var(--text-secondary)] bg-[var(--bg-secondary)]';
    }
  };

  if (isBackgrounded) {
    return (
      <div className="fixed bottom-4 right-4 z-50">
        <div className="bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg shadow-lg p-4 w-80">
          <div className="flex items-center gap-3">
            <div className={`w-10 h-10 rounded-full flex items-center justify-center ${getStatusColor()}`}>
              {getStatusIcon()}
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-[var(--text-primary)] truncate">
                {getStatusMessage()}
              </p>
              {progress && (
                <div className="w-full bg-[var(--bg-secondary)] rounded-full h-1.5 mt-1">
                  <div
                    className="bg-[var(--accent-primary)] h-1.5 rounded-full transition-all duration-300"
                    style={{ width: `${progress.percentage}%` }}
                  />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    );
  }

  const content = (
    <div className={embedded ? '' : 'bg-[var(--surface-elevated)] rounded-lg p-8 max-w-lg w-full shadow-xl'}>
      <div className="text-center mb-6">
        <div className={`w-16 h-16 rounded-full flex items-center justify-center mx-auto mb-4 ${getStatusColor()} ${status === 'downloading' ? 'animate-pulse' : ''}`}>
          {getStatusIcon()}
        </div>
        <h2 className="text-2xl font-semibold text-[var(--text-primary)] mb-2">
          {status === 'error' ? 'Download Failed' : 'Downloading AI Models'}
        </h2>
        <p className="text-sm text-[var(--text-secondary)]">{getStatusMessage()}</p>
      </div>

      {status === 'error' && error && (
        <div className="bg-[var(--error-light)] border border-[var(--error)] rounded-lg p-4 mb-6 animate-in fade-in slide-in-from-top-2 duration-200">
          <p className="text-sm text-[var(--error)] font-medium mb-2">Error Details:</p>
          <p className="text-sm text-[var(--error)]">{error}</p>
        </div>
      )}

      {progress && status !== 'error' && (
        <div className="space-y-4">
          <div>
            <div className="flex justify-between items-baseline mb-2">
              <span className="text-sm font-medium text-[var(--text-secondary)]">
                {progress.current_file}
              </span>
              <span className="text-lg font-bold text-[var(--accent-primary)]">
                {progress.percentage.toFixed(1)}%
              </span>
            </div>
            <div className="w-full bg-[var(--bg-secondary)] rounded-full h-3 overflow-hidden">
              <div
                className="gradient-brand h-3 rounded-full transition-all duration-300 ease-out"
                style={{ width: `${progress.percentage}%` }}
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 pt-4 border-t border-[var(--border-color)]">
            <div>
              <p className="text-xs text-[var(--text-tertiary)] mb-1">Downloaded</p>
              <p className="text-sm font-semibold text-[var(--text-primary)]">
                {formatBytes(progress.bytes_downloaded)}
                <span className="text-[var(--text-tertiary)] font-normal">
                  {' '}
                  / {formatBytes(progress.total_bytes)}
                </span>
              </p>
            </div>
            <div>
              <p className="text-xs text-[var(--text-tertiary)] mb-1">Speed</p>
              <p className="text-sm font-semibold text-[var(--text-primary)]">
                {progress.speed_mbps.toFixed(2)} MB/s
              </p>
            </div>
            <div>
              <p className="text-xs text-[var(--text-tertiary)] mb-1">Time Remaining</p>
              <p className="text-sm font-semibold text-[var(--text-primary)]">
                {formatTime(progress.eta_seconds)}
              </p>
            </div>
            <div>
              <p className="text-xs text-[var(--text-tertiary)] mb-1">Status</p>
              <p className="text-sm font-semibold text-[var(--success)]">
                {status === 'complete' ? 'Complete' : 'In Progress'}
              </p>
            </div>
          </div>
        </div>
      )}

      {!progress && status === 'initializing' && (
        <div className="flex items-center justify-center py-12">
          <div className="animate-spin rounded-full h-10 w-10 border-b-2 border-[var(--accent-primary)]" />
          <span className="ml-3 text-[var(--text-secondary)]">Initializing download...</span>
        </div>
      )}

      {status === 'complete' && (
        <div className="text-center py-8 animate-in fade-in zoom-in-95 duration-300">
          <CheckCircle2 className="w-16 h-16 text-[var(--success)] mx-auto mb-3" />
          <p className="text-lg font-semibold text-[var(--text-primary)]">Models ready!</p>
          <p className="text-sm text-[var(--text-secondary)] mt-1">
            Proceeding to next step...
          </p>
        </div>
      )}

      <div className="mt-6 pt-6 border-t border-[var(--border-color)]">
        {status === 'error' ? (
          <div className="flex gap-3">
            <button
              onClick={handleRetry}
              className="flex-1 px-4 py-3 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] transition-colors font-medium flex items-center justify-center gap-2"
              aria-label="Retry download"
            >
              <RefreshCw className="w-4 h-4" />
              Retry Download
            </button>
          </div>
        ) : (
          <div>
            {allowBackground && status === 'downloading' && (
              <button
                onClick={handleBackground}
                className="w-full px-4 py-2 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors flex items-center justify-center gap-2"
                aria-label="Continue in background"
              >
                <Minimize2 className="w-4 h-4" />
                Continue in Background
              </button>
            )}
            <p className="text-xs text-[var(--text-tertiary)] text-center mt-3">
              Downloading embedding model from HuggingFace (~90MB)
              <br />
              This only happens once on first launch
            </p>
          </div>
        )}
      </div>
    </div>
  );

  if (embedded) {
    return content;
  }

  return (
    <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      {content}
    </div>
  );
}
