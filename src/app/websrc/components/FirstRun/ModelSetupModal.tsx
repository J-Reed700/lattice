import { useState, useEffect } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { Download, Sparkles, CheckCircle, XCircle } from 'lucide-react';

import { useDownloads } from '../../hooks/useDownloads';
import { getErrorMessage } from '../../lib/errorUtils';
import { toast } from '../../stores/toastStore';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Progress } from '../ui/progress';

interface ModelSetupModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onComplete: () => void;
}

interface FirstRunStatusResponse {
  needs_setup: boolean;
  recommended_model_id: string | null;
  recommended_model_name: string | null;
  estimated_size_bytes: number | null;
}

interface DownloadDefaultModelResponse {
  download_id: string;
  model_id: string;
  model_name: string;
  file_path: string;
  file_size_bytes: number;
}

function formatFileSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${mb.toFixed(2)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function ModelSetupModal({ open, onOpenChange, onComplete }: ModelSetupModalProps) {
  const [loading, setLoading] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadId, setDownloadId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [modelInfo, setModelInfo] = useState<FirstRunStatusResponse | null>(null);

  const { getDownload } = useDownloads();

  useEffect(() => {
    if (open) {
      checkFirstRunStatus();
    }
  }, [open]);

  const checkFirstRunStatus = async () => {
    try {
      setLoading(true);
      setError(null);
      const statusJson = await invoke<string>('plugin:model|check_first_run_status');
      const status: FirstRunStatusResponse = JSON.parse(statusJson);
      setModelInfo(status);
    } catch (err) {
      console.error('Failed to check first-run status:', err);
      setError(getErrorMessage(err));
    } finally {
      setLoading(false);
    }
  };

  const handleDownload = async () => {
    try {
      setDownloading(true);
      setError(null);

      const responseJson = await invoke<string>('plugin:model|download_default_embedding_model');
      const response: DownloadDefaultModelResponse = JSON.parse(responseJson);
      setDownloadId(response.download_id);

      toast.success('Model download started', {
        message: `Downloading ${response.model_name} (~${formatFileSize(response.file_size_bytes)})`,
      });
    } catch (err) {
      console.error('Failed to start download:', err);
      const errorMessage = getErrorMessage(err);
      setError(errorMessage);
      setDownloading(false);
      toast.error('Download failed', {
        message: errorMessage,
      });
    }
  };

  const handleSkip = () => {
    localStorage.setItem('recall:first-run-skipped', 'true');
    onOpenChange(false);
    onComplete();
  };

  const download = downloadId ? getDownload(downloadId) : null;

  useEffect(() => {
    if (download?.status === 'completed') {
      setDownloading(false);
      toast.success('Model downloaded successfully!', {
        message: 'Your embedding model is ready to use.',
      });
      onOpenChange(false);
      onComplete();
    } else if (download?.status === 'error') {
      setDownloading(false);
      setError('Download failed');
      toast.error('Download failed', {
        message: 'An error occurred during download',
      });
    }
  }, [download]);

  const progressPercentage = download?.kind === 'single' && download.totalBytes && download.totalBytes > 0
    ? (download.bytesDownloaded / download.totalBytes) * 100
    : download?.kind === 'batch' && download.aggregateTotalBytes > 0
      ? download.aggregatePercentage
      : 0;

  if (!modelInfo) {
    return null;
  }

  if (!modelInfo.needs_setup) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-purple-600 dark:text-purple-400" />
            Welcome to Recall
          </DialogTitle>
          <DialogDescription>
            To enable semantic search, you need an embedding model.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {loading && (
            <div className="flex items-center justify-center py-8">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-purple-600" />
            </div>
          )}

          {!loading && !downloading && (
            <>
              <div className="bg-purple-50 dark:bg-purple-950/20 p-4 rounded-lg border border-purple-200 dark:border-purple-800">
                <div className="flex items-start gap-3">
                  <Download className="w-5 h-5 text-purple-600 dark:text-purple-400 mt-0.5" />
                  <div className="flex-1">
                    <h3 className="font-semibold text-sm text-[var(--text-primary)] mb-1">
                      {modelInfo.recommended_model_name || 'Recommended Model'}
                    </h3>
                    <p className="text-xs text-[var(--text-secondary)] mb-2">
                      A lightweight, high-quality embedding model for semantic search
                    </p>
                    <div className="flex items-center gap-2 text-xs text-[var(--text-tertiary)]">
                      <span>Size: {modelInfo.estimated_size_bytes ? formatFileSize(modelInfo.estimated_size_bytes) : 'Unknown'}</span>
                      <span>•</span>
                      <span>Source: HuggingFace</span>
                    </div>
                  </div>
                </div>
              </div>

              {error && (
                <div className="bg-red-50 dark:bg-red-950/20 p-4 rounded-lg border border-red-200 dark:border-red-800">
                  <div className="flex items-start gap-3">
                    <XCircle className="w-5 h-5 text-red-600 dark:text-red-400 mt-0.5" />
                    <div className="flex-1">
                      <h3 className="font-semibold text-sm text-red-800 dark:text-red-200 mb-1">
                        Download Failed
                      </h3>
                      <p className="text-xs text-red-700 dark:text-red-300">
                        {error}
                      </p>
                    </div>
                  </div>
                </div>
              )}
            </>
          )}

          {downloading && download?.kind === 'single' && (
            <div className="space-y-3">
              <div className="bg-blue-50 dark:bg-blue-950/20 p-4 rounded-lg border border-blue-200 dark:border-blue-800">
                <div className="flex items-start gap-3">
                  <CheckCircle className="w-5 h-5 text-blue-600 dark:text-blue-400 mt-0.5" />
                  <div className="flex-1">
                    <h3 className="font-semibold text-sm text-[var(--text-primary)] mb-1">
                      Downloading Model
                    </h3>
                    <p className="text-xs text-[var(--text-secondary)] mb-3">
                      {formatFileSize(download.bytesDownloaded)} of {formatFileSize(download.totalBytes || 0)}
                    </p>
                    <Progress value={progressPercentage} className="h-2" />
                    <div className="flex items-center justify-between mt-2 text-xs text-[var(--text-tertiary)]">
                      <span>{progressPercentage.toFixed(1)}%</span>
                      <span>
                        {download.bytesPerSecond > 0
                          ? `${formatFileSize(download.bytesPerSecond)}/s`
                          : 'Calculating...'}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {downloading && download?.kind === 'batch' && (
            <div className="space-y-3">
              <div className="bg-blue-50 dark:bg-blue-950/20 p-4 rounded-lg border border-blue-200 dark:border-blue-800">
                <div className="flex items-start gap-3">
                  <CheckCircle className="w-5 h-5 text-blue-600 dark:text-blue-400 mt-0.5" />
                  <div className="flex-1">
                    <h3 className="font-semibold text-sm text-[var(--text-primary)] mb-1">
                      Downloading Model Files
                    </h3>
                    <p className="text-xs text-[var(--text-secondary)] mb-3">
                      {download.completedFiles} of {download.totalFiles} files • {formatFileSize(download.aggregateBytesDownloaded)} of {formatFileSize(download.aggregateTotalBytes)}
                    </p>
                    <Progress value={progressPercentage} className="h-2" />
                    <div className="flex items-center justify-between mt-2 text-xs text-[var(--text-tertiary)]">
                      <span>{progressPercentage.toFixed(1)}%</span>
                      <span>
                        {download.aggregateBytesPerSecond > 0
                          ? `${formatFileSize(download.aggregateBytesPerSecond)}/s`
                          : 'Calculating...'}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="flex-col sm:flex-row gap-2">
          {!downloading && (
            <>
              <Button
                variant="ghost"
                onClick={handleSkip}
                disabled={loading}
              >
                Skip for Now
              </Button>
              <Button
                onClick={handleDownload}
                disabled={loading || error !== null}
                className="bg-purple-600 hover:bg-purple-700"
              >
                <Download className="w-4 h-4 mr-2" />
                Download Now
              </Button>
            </>
          )}
          {downloading && (
            <div className="text-xs text-[var(--text-tertiary)] text-center">
              Please wait while the model downloads...
            </div>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
