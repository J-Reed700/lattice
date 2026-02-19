import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Activity, FolderPlus, Settings } from 'lucide-react';
import { IndexProgress } from '../IndexProgress';
import { FolderList } from '../FolderList';
import VaultAPI from '../../lib/api';
import { useError } from '../../contexts/ErrorContext';
import { useTauriCommand } from '../../hooks/useTauriCommand';
import type { IndexingActivity, IndexProgress as IndexProgressType } from '../../types';

interface IndexingPanelProps {
  className?: string;
}

export function IndexingPanel({ className = '' }: IndexingPanelProps) {
  const [activeTab, setActiveTab] = useState<'folders' | 'activity'>('folders');
  const [isIndexing, setIsIndexing] = useState(false);
  const [currentProgress, setCurrentProgress] = useState<IndexProgressType | null>(null);
  const [activities, setActivities] = useState<IndexingActivity[]>([]);
  const [showSettings, setShowSettings] = useState(false);

  const { handleTauriError } = useError();

  const {
    execute: startIndexing,
    loading: isStartingIndex,
    error: indexError,
  } = useTauriCommand(VaultAPI.startIndexing, {
    onSuccess: () => {
      setIsIndexing(true);
    },
    errorMessage: 'Failed to start indexing',
  });

  const {
    execute: cancelIndexing,
    loading: isCancelling,
  } = useTauriCommand(VaultAPI.cancelIndexing, {
    onSuccess: () => {
      setIsIndexing(false);
    },
    errorMessage: 'Failed to cancel indexing',
  });

  const loadActivities = async () => {
    try {
      const result = await VaultAPI.getIndexingActivities(10);
      if (result.ok) {
        setActivities(result.data);
      } else {
        handleTauriError(result.error || 'Failed to load activities', {
          severity: 'warning',
          recoverable: true,
        });
      }
    } catch (error) {
      handleTauriError(
        error instanceof Error ? error.message : 'Failed to load activities',
        { severity: 'warning', recoverable: true }
      );
    }
  };

  useEffect(() => {
    loadActivities();
  }, []);

  useEffect(() => {
    const unlistenProgress = listen<IndexProgressType>('indexing-progress', (event) => {
      setCurrentProgress(event.payload);
      setIsIndexing(event.payload.status === 'scanning' || event.payload.status === 'processing');
    });

    const unlistenComplete = listen('indexing-complete', () => {
      setIsIndexing(false);
      loadActivities();
    });

    const unlistenError = listen<string>('indexing-error', (event) => {
      setIsIndexing(false);
      handleTauriError(event.payload, {
        severity: 'error',
        recoverable: true,
        action: {
          label: 'Retry',
          onClick: handleQuickAddFolder,
        },
      });
      loadActivities();
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const handleQuickAddFolder = async () => {
    try {
      const folderPath = await VaultAPI.selectFolder();
      if (!folderPath) return;

      await startIndexing(folderPath, true);
    } catch (error) {
      handleTauriError(
        error instanceof Error ? error.message : 'Failed to select folder',
        {
          severity: 'error',
          recoverable: true,
          action: {
            label: 'Try Again',
            onClick: handleQuickAddFolder,
          },
        }
      );
    }
  };

  const handleCancelIndexing = async () => {
    await cancelIndexing();
  };

  return (
    <div className={`flex flex-col h-full ${className}`}>
      <div className="p-6 border-b border-[var(--border-color)]">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-semibold text-[var(--text-primary)]">Indexing</h2>
          <button
            onClick={() => setShowSettings(!showSettings)}
            className="p-2 rounded-lg hover:bg-[var(--surface-hover)] transition-colors"
            aria-label="Indexing settings"
          >
            <Settings className="w-5 h-5 text-[var(--text-secondary)]" />
          </button>
        </div>

        {currentProgress && isIndexing && (
          <div className="mb-4">
            <IndexProgress
              progress={currentProgress}
              onCancel={handleCancelIndexing}
              isCancelling={isCancelling}
            />
          </div>
        )}

        <button
          onClick={handleQuickAddFolder}
          disabled={isIndexing || isStartingIndex}
          className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <FolderPlus className="w-5 h-5" />
          {isStartingIndex ? 'Starting...' : 'Add Folder to Index'}
        </button>

        {indexError && (
          <div className="mt-3 p-3 bg-[var(--error-light)]/20 border border-[var(--error-light)] rounded-lg">
            <p className="text-sm text-[var(--error)]">{indexError}</p>
          </div>
        )}
      </div>

      <div className="flex-1 overflow-hidden flex flex-col">
        <div className="border-b border-[var(--border-color)]">
          <div className="flex">
            <button
              onClick={() => setActiveTab('folders')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
                activeTab === 'folders'
                  ? 'text-[var(--accent-primary)] border-b-2 border-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
              }`}
            >
              Folders
            </button>
            <button
              onClick={() => setActiveTab('activity')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
                activeTab === 'activity'
                  ? 'text-[var(--accent-primary)] border-b-2 border-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
              }`}
            >
              <div className="flex items-center justify-center gap-2">
                <Activity className="w-4 h-4" />
                Activity
              </div>
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto">
          {activeTab === 'folders' ? (
            <FolderList onFolderAdded={loadActivities} />
          ) : (
            <ActivityLog activities={activities} />
          )}
        </div>
      </div>
    </div>
  );
}

function ActivityLog({ activities }: { activities: IndexingActivity[] }) {
  if (activities.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-[var(--text-secondary)] p-8">
        <Activity className="w-12 h-12 mb-3 opacity-50" />
        <p className="text-sm">No recent activity</p>
      </div>
    );
  }

  return (
    <div className="p-4 space-y-2">
      {activities.map((activity) => (
        <div
          key={activity.id}
          className="p-3 bg-[var(--surface-elevated)] rounded-lg border border-[var(--border-color)]"
        >
          <div className="flex items-start justify-between">
            <div className="flex-1">
              <p className="text-sm font-medium text-[var(--text-primary)]">{activity.path}</p>
              <p className="text-xs text-[var(--text-secondary)] mt-1">{activity.timestamp}</p>
            </div>
            <span
              className={`text-xs px-2 py-1 rounded ${
                activity.status === 'completed'
                  ? 'bg-[var(--success-light)] text-[var(--success)]'
                  : activity.status === 'failed'
                  ? 'bg-[var(--error-light)] text-[var(--error)]'
                  : 'bg-[var(--accent-light)] text-[var(--accent-primary)]'
              }`}
            >
              {activity.status}
            </span>
          </div>
          {activity.error && (
            <p className="text-xs text-[var(--error)] mt-2">{activity.error}</p>
          )}
        </div>
      ))}
    </div>
  );
}
