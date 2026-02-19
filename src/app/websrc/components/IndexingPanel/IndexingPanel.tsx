/**
 * IndexingPanel Component
 *
 * Purpose: Central control panel for managing document indexing operations
 *
 * Features:
 * - Folder picker and management
 * - Real-time indexing progress
 * - List of indexed folders
 * - Recent indexing activity log
 * - Complete control over indexing lifecycle
 *
 * This component integrates:
 * - IndexProgress: Shows current indexing status
 * - FolderList: Manages indexed folders
 * - Activity Log: Shows recent operations
 *
 * States: idle, indexing, complete, error
 * Accessibility: WCAG AA, keyboard navigation, screen reader support
 */

import { useState, useEffect } from 'react';

import { Activity, FolderPlus} from 'lucide-react';

import VaultAPI from '../../lib/api';
import { TauriEventNames, EventSchemas, listenValidated } from '../../types/events';
import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { FolderList } from '../FolderList';
import { IndexProgress } from '../IndexProgress';

import type { IndexingActivity, IndexProgress as IndexProgressType } from '../../types';

interface IndexingPanelProps {
  /** Custom CSS classes */
  className?: string;
}

export function IndexingPanel({ className = '' }: IndexingPanelProps) {
  const [activeTab, setActiveTab] = useState<'folders' | 'activity'>('folders');
  const [isIndexing, setIsIndexing] = useState(false);
  const [currentProgress, setCurrentProgress] = useState<IndexProgressType | null>(null);
  const [activities, setActivities] = useState<IndexingActivity[]>([]);

  // Load recent activities
  const loadActivities = async () => {
    const result = await VaultAPI.getIndexingActivities(10);
    if (result.ok) {
      setActivities(result.data);
    }
  };

  useEffect(() => {
    void loadActivities();
  }, []);

  // Listen for indexing events
  useEffect(() => {
    const unlisten = listenValidated(
      TauriEventNames.Indexing.Event,
      EventSchemas.Indexing.Event,
      (event) => {
        const payload = event.payload;

        // Handle different event types
        switch (payload.type) {
          case 'Started':
            setCurrentProgress({
              totalFiles: payload.total_files,
              processed: 0,
              failed: 0,
              currentFile: undefined,
              status: 'scanning',
              percentage: 0,
              estimatedRemainingMs: undefined,
            });
            setIsIndexing(true);
            break;

          case 'FileStarted':
            setCurrentProgress((prev) => ({
              totalFiles: payload.total,
              processed: payload.current - 1,
              failed: prev?.failed || 0,
              currentFile: payload.path,
              status: 'processing',
              percentage: ((payload.current - 1) / payload.total) * 100,
              estimatedRemainingMs: undefined,
            }));
            setIsIndexing(payload.current < payload.total);
            break;

          case 'FileCompleted':
            setCurrentProgress((prev) => ({
              totalFiles: payload.total,
              processed: payload.current,
              failed: prev?.failed || 0,
              currentFile: payload.path,
              status: 'processing',
              percentage: (payload.current / payload.total) * 100,
              estimatedRemainingMs: undefined,
            }));
            setIsIndexing(payload.current < payload.total);
            break;

          case 'FileError':
            setCurrentProgress((prev) => ({
              totalFiles: payload.total,
              processed: prev?.processed || 0,
              failed: (prev?.failed || 0) + 1,
              currentFile: payload.path,
              status: 'error',
              percentage: (payload.current / payload.total) * 100,
              estimatedRemainingMs: undefined,
            }));
            setIsIndexing(true);
            break;

          case 'Completed':
            setCurrentProgress((prev) => ({
              totalFiles: payload.total_files,
              processed: payload.total_files,
              failed: prev?.failed || 0,
              currentFile: undefined,
              status: 'complete',
              percentage: 100,
              estimatedRemainingMs: undefined,
            }));
            setIsIndexing(false);
            void loadActivities(); // Reload activities to show completion
            break;

          case 'Cancelled':
            setCurrentProgress((prev) => ({
              totalFiles: prev?.totalFiles || 0,
              processed: prev?.processed || 0,
              failed: prev?.failed || 0,
              currentFile: undefined,
              status: 'cancelled',
              percentage: prev?.percentage || 0,
              estimatedRemainingMs: undefined,
            }));
            setIsIndexing(false);
            void loadActivities();
            break;
        }
      },
      (error) => {
        console.error(
          '[IndexingPanel] Validation error for indexing event:',
          error.format()
        );
      }
    );

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  // Handle quick add folder
  const handleQuickAddFolder = async () => {
    const folderPath = await VaultAPI.selectFolder();

    if (!folderPath) return;

    const result = await VaultAPI.startIndexing(folderPath);

    if (result.ok) {
      setIsIndexing(true);
    } else {
      alert(`Failed to start indexing: ${result.error}`);
    }
  };

  // Format activity timestamp
  const formatActivityTime = (timestamp: string): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date >= today) {
      return date.toLocaleTimeString(undefined, {
        hour: 'numeric',
        minute: '2-digit'
      });
    } else if (date >= yesterday) {
      return `Yesterday ${date.toLocaleTimeString(undefined, {
        hour: 'numeric',
        minute: '2-digit'
      })}`;
    } else {
      return date.toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit'
      });
    }
  };

  // Get activity icon and color based on type and status
  const getActivityStyle = (activity: IndexingActivity) => {
    if (activity.status === 'error') {
      return {
        icon: '✕',
        bgColor: 'bg-[var(--error-light)]/30',
        textColor: 'text-[var(--error)]',
      };
    }
    if (activity.status === 'success') {
      return {
        icon: '✓',
        bgColor: 'bg-[var(--success-light)]/30',
        textColor: 'text-[var(--success)]',
      };
    }
    return {
      icon: '○',
      bgColor: 'bg-[var(--accent-light)]/30',
      textColor: 'text-[var(--accent-primary)]',
    };
  };

  return (
    <div className={`flex flex-col h-full ${className}`}>
      {/* Header */}
      <div className="flex-shrink-0 px-6 py-4 border-b border-[var(--border-color)]">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-xl font-semibold text-[var(--text-primary)]">
              Document Indexing
            </h2>
            <p className="text-sm text-[var(--text-secondary)] mt-1">
              Manage folders and track indexing progress
            </p>
          </div>

          {/* Quick add folder button */}
          <button
            onClick={handleAsyncEvent(handleQuickAddFolder)}
            disabled={isIndexing}
            className="inline-flex items-center gap-2 px-4 py-2 bg-[var(--accent-primary)] hover:bg-[var(--accent-hover)] disabled:bg-[var(--bg-tertiary)] disabled:cursor-not-allowed text-white rounded-lg font-medium transition-colors"
            aria-label="Add folder to index"
          >
            <FolderPlus className="w-4 h-4" />
            Add Folder
          </button>
        </div>
      </div>

      {/* Current indexing progress (if active) */}
      {isIndexing && currentProgress && (
        <div className="flex-shrink-0 px-6 py-4 bg-[var(--accent-light)]/10 border-b border-[var(--accent-light)]">
          <IndexProgress
            onComplete={() => setIsIndexing(false)}
            onCancel={() => setIsIndexing(false)}
          />
        </div>
      )}

      {/* Tabs */}
      <div className="flex-shrink-0 border-b border-[var(--border-color)]">
        <nav className="flex gap-6 px-6" aria-label="Indexing sections">
          <button
            onClick={() => setActiveTab('folders')}
            className={`py-3 border-b-2 font-medium text-sm transition-colors ${
              activeTab === 'folders'
                ? 'border-[var(--accent-primary)] text-[var(--accent-primary)]'
                : 'border-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-color)]'
            }`}
            aria-current={activeTab === 'folders' ? 'page' : undefined}
          >
            Indexed Folders
          </button>
          <button
            onClick={() => setActiveTab('activity')}
            className={`py-3 border-b-2 font-medium text-sm transition-colors ${
              activeTab === 'activity'
                ? 'border-[var(--accent-primary)] text-[var(--accent-primary)]'
                : 'border-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-color)]'
            }`}
            aria-current={activeTab === 'activity' ? 'page' : undefined}
          >
            <span className="flex items-center gap-2">
              <Activity className="w-4 h-4" />
              Recent Activity
            </span>
          </button>
        </nav>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-y-auto">
        {activeTab === 'folders' && (
          <FolderList
            onFolderAdded={() => {
              setIsIndexing(true);
              loadActivities();
            }}
            onFolderRemoved={() => {
              loadActivities();
            }}
            onReindexRequested={() => {
              setIsIndexing(true);
              loadActivities();
            }}
          />
        )}

        {activeTab === 'activity' && (
          <div className="p-6">
            {activities.length === 0 ? (
              // Empty state
              <div className="text-center py-12">
                <div className="inline-flex items-center justify-center w-16 h-16 bg-[var(--bg-primary)] rounded-full mb-4">
                  <Activity className="w-8 h-8 text-[var(--text-tertiary)]" />
                </div>
                <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-2">
                  No activity yet
                </h3>
                <p className="text-sm text-[var(--text-secondary)]">
                  Index your first folder to see activity history
                </p>
              </div>
            ) : (
              // Activity list
              <div className="space-y-1">
                <h3 className="text-sm font-semibold text-[var(--text-primary)] mb-3 px-1">
                  Last 10 Operations
                </h3>
                {activities.map((activity) => {
                  const style = getActivityStyle(activity);
                  return (
                    <div
                      key={activity.id}
                      className="flex items-start gap-3 p-3 rounded-lg hover:bg-[var(--surface-hover)]/50 transition-colors"
                    >
                      {/* Status icon */}
                      <div className={`flex-shrink-0 w-8 h-8 ${style.bgColor} rounded-full flex items-center justify-center mt-0.5`}>
                        <span className={`text-sm font-bold ${style.textColor}`}>
                          {style.icon}
                        </span>
                      </div>

                      {/* Activity details */}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-baseline justify-between gap-2">
                          <p className="text-sm font-medium text-[var(--text-primary)]">
                            {activity.action}
                          </p>
                          <span className="text-xs text-[var(--text-tertiary)] flex-shrink-0">
                            {formatActivityTime(activity.timestamp)}
                          </span>
                        </div>
                        <p className="text-xs text-[var(--text-secondary)] mt-1 truncate">
                          {activity.file_path}
                        </p>
                        {activity.details && (
                          <p className="text-xs text-[var(--text-tertiary)] mt-1">
                            {activity.details}
                          </p>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer with stats or settings */}
      <div className="flex-shrink-0 px-6 py-3 border-t border-[var(--border-color)] bg-[var(--bg-secondary)]/50">
        <div className="flex items-center justify-between text-xs text-[var(--text-secondary)]">
          <span>
            Vault automatically monitors indexed folders for changes
          </span>
          {/* Settings button for future features */}
          {/* <button
            onClick={() => setShowSettings(!showSettings)}
            className="p-1 hover:bg-[var(--surface-hover)] rounded transition-colors"
            aria-label="Indexing settings"
          >
            <Settings className="w-4 h-4" />
          </button> */}
        </div>
      </div>
    </div>
  );
}
