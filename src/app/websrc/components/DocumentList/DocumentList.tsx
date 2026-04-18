import { useState, useEffect, useCallback, memo } from 'react';

import { useInterval } from '../../hooks';
import VaultAPI from '../../lib/api';
import { handleAsyncEvent } from '../../utils/promiseHandlers';

import type { IndexingStats, IndexProgress } from '../../types';

const StatCard = memo(({ title, value, color }: { title: string; value: number; color: string }) => (
  <div className="bg-[hsl(var(--surface-raised))] p-6 rounded-lg shadow">
    <h3 className="text-lg font-semibold text-[hsl(var(--text-secondary))] mb-2">{title}</h3>
    <p className={`text-3xl font-bold ${color}`}>{value}</p>
  </div>
));

StatCard.displayName = 'StatCard';

interface ProgressDisplayProps {
  progress: IndexProgress;
  onCancel: () => void;
  formatTime: (ms?: number) => string;
}

const ProgressDisplay = memo(({ progress, onCancel, formatTime }: ProgressDisplayProps) => (
  <div className="bg-[hsl(var(--surface-raised))] p-6 rounded-lg shadow">
    <div className="flex items-center justify-between mb-4">
      <h3 className="text-lg font-semibold text-[hsl(var(--text-secondary))]">Indexing Progress</h3>
      {progress.status === 'processing' && (
        <button
          onClick={onCancel}
          className="px-3 py-1 text-sm bg-[hsl(var(--danger-fg))] text-[hsl(var(--text-primary))] rounded hover:bg-[hsl(var(--danger-fg))] hover:opacity-90 transition-colors duration-fast"
        >
          Cancel
        </button>
      )}
    </div>

    <div className="mb-4">
      <div className="flex justify-between text-sm text-[hsl(var(--text-secondary))] mb-2">
        <span className="capitalize">{progress.status}</span>
        <span>{Math.round(progress.percentage)}%</span>
      </div>
      <div className="w-full bg-[hsl(var(--surface-raised))] rounded-full h-3">
        <div
          className={`h-3 rounded-full transition-colors duration-fast ${
            progress.status === 'complete'
              ? 'bg-[hsl(var(--success-fg))]'
              : progress.status === 'error'
              ? 'bg-[hsl(var(--danger-fg))]'
              : progress.status === 'cancelled'
              ? 'bg-[hsl(var(--text-secondary))]'
              : 'bg-[hsl(var(--accent))]'
          }`}
          style={{ width: `${progress.percentage}%` }}
         />
      </div>
    </div>

    <div className="space-y-2 text-sm text-[hsl(var(--text-secondary))]">
      <div className="flex justify-between">
        <span>Files processed:</span>
        <span className="font-medium">
          {progress.processed} / {progress.totalFiles}
        </span>
      </div>
      {progress.failed > 0 && (
        <div className="flex justify-between text-[hsl(var(--danger-fg))]">
          <span>Failed:</span>
          <span className="font-medium">{progress.failed}</span>
        </div>
      )}
      {progress.currentFile && (
        <div className="mt-2">
          <span className="text-[hsl(var(--text-secondary))]">Current file:</span>
          <p className="text-xs text-[hsl(var(--text-secondary))] truncate mt-1">
            {progress.currentFile}
          </p>
        </div>
      )}
      {progress.estimatedRemainingMs && (
        <div className="flex justify-between text-[hsl(var(--accent))]">
          <span>{formatTime(progress.estimatedRemainingMs)}</span>
        </div>
      )}
    </div>
  </div>
));

ProgressDisplay.displayName = 'ProgressDisplay';

export function DocumentList() {
  const [stats, setStats] = useState<IndexingStats | null>(null);
  const [progress, setProgress] = useState<IndexProgress | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadStats = useCallback(async () => {
    setIsLoading(true);
    const result = await VaultAPI.getIndexingStats();
    if (result.ok) {
      setStats(result.data);
      setError(null);
    } else {
      setError(result.error);
    }
    setIsLoading(false);
  }, []);

  const loadProgress = useCallback(async () => {
    const result = await VaultAPI.getIndexProgress();
    if (result.ok) {
      setProgress(result.data);
    }
  }, []);

  // Load stats on mount
  useEffect(() => {
    void loadStats();
    void loadProgress();
  }, [loadStats, loadProgress]);

  // Poll progress every second
  useInterval(
    () => {
      void loadProgress();
    },
    1000
  );

  const handleCancelIndexing = useCallback(async () => {
    const result = await VaultAPI.cancelIndexing();
    if (result.ok) {
      await loadProgress();
    }
  }, [loadProgress]);

  const formatEstimatedTime = useCallback((ms?: number) => {
    if (!ms) return '';
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (hours > 0) return `${hours}h ${minutes % 60}m remaining`;
    if (minutes > 0) return `${minutes}m ${seconds % 60}s remaining`;
    return `${seconds}s remaining`;
  }, []);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-[hsl(var(--accent))]" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4 bg-[hsl(var(--danger-muted))]/20 border border-[hsl(var(--danger-muted))] rounded-lg">
        <p className="text-[hsl(var(--danger-fg))]">{error}</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <StatCard
          title="Indexed Documents"
          value={stats?.indexedDocuments ?? 0}
          color="text-[hsl(var(--accent))]"
        />
        <StatCard
          title="Total Chunks"
          value={stats?.totalChunks ?? 0}
          color="text-[hsl(var(--success-fg))]"
        />
      </div>

      {progress && progress.status !== 'idle' && (
        <ProgressDisplay
          progress={progress}
          onCancel={handleAsyncEvent(handleCancelIndexing)}
          formatTime={formatEstimatedTime}
        />
      )}

      <div className="bg-[hsl(var(--surface-raised))] p-6 rounded-lg shadow">
        <h3 className="text-lg font-semibold text-[hsl(var(--text-secondary))] mb-4">Actions</h3>
        <div className="flex gap-3">
          <button
            onClick={handleAsyncEvent(loadStats)}
            className="px-4 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-lg hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast"
          >
            Refresh Stats
          </button>
        </div>
      </div>
    </div>
  );
}
