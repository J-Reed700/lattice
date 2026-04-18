import { Activity, FileText, RefreshCw, Trash2, Clock } from 'lucide-react';

import Card, { CardHeader, CardTitle, CardContent } from '../ui/Card/Card';

import type { IndexingActivity } from '../../types';

/**
 * RecentActivity
 *
 * Purpose: Display recent indexing operations and system activity
 *
 * Features:
 * - Chronological activity feed
 * - Action type indicators (indexed, reindexed, removed)
 * - Timestamps (relative time)
 * - File path truncation for long paths
 * - Status indicators (success, error)
 * - Empty state for no activity
 *
 * States: loading, empty, populated
 * Accessibility: WCAG AA, semantic HTML, screen reader friendly
 */

interface RecentActivityProps {
  activities: IndexingActivity[];
  loading?: boolean;
}

export const RecentActivity = ({ activities, loading = false }: RecentActivityProps) => {
  const getActionIcon = (action: string) => {
    switch (action.toLowerCase()) {
      case 'indexed':
      case 'index':
        return FileText;
      case 'reindexed':
      case 'reindex':
        return RefreshCw;
      case 'removed':
      case 'remove':
        return Trash2;
      default:
        return Activity;
    }
  };

  const getActionColor = (status: string) => {
    switch (status.toLowerCase()) {
      case 'success':
      case 'completed':
        return 'text-[hsl(var(--success-fg))]';
      case 'error':
      case 'failed':
        return 'text-[hsl(var(--danger-fg))]';
      case 'pending':
      case 'processing':
        return 'text-[hsl(var(--warning-fg))]';
      default:
        return 'text-[hsl(var(--text-secondary))]';
    }
  };

  const getRelativeTime = (timestamp: string): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;

    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;

    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;

    return date.toLocaleDateString();
  };

  const truncatePath = (path: string, maxLength: number = 30): string => {
    if (path.length <= maxLength) return path;

    const fileName = path.split(/[\\/]/).pop() || path;
    if (fileName.length >= maxLength - 3) {
      return `...${fileName.slice(-(maxLength - 3))}`;
    }

    const remainingLength = maxLength - fileName.length - 4; // 4 for ".../"
    const pathStart = path.slice(0, remainingLength);
    return `${pathStart}.../\u200B${fileName}`;
  };

  if (loading) {
    return (
      <Card padding="md" className="h-full">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="w-5 h-5" />
            Recent Activity
          </CardTitle>
        </CardHeader>
        <CardContent className="mt-4 space-y-3">
          {[...Array(5)].map((_, i) => (
            <div key={i} className="animate-pulse flex items-start gap-3">
              <div className="w-8 h-8 bg-[hsl(var(--surface-raised))] rounded-full" />
              <div className="flex-1 space-y-2">
                <div className="h-4 bg-[hsl(var(--surface-raised))] rounded w-3/4" />
                <div className="h-3 bg-[hsl(var(--surface-raised))] rounded w-1/2" />
              </div>
            </div>
          ))}
        </CardContent>
      </Card>
    );
  }

  return (
    <Card padding="md" className="h-full flex flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-[hsl(var(--text-primary))]">
          <Activity className="w-5 h-5 text-[hsl(var(--accent))]" />
          Recent Activity
        </CardTitle>
      </CardHeader>

      <CardContent className="mt-4 flex-1 overflow-auto">
        {activities.length === 0 ? (
          <div className="text-center py-8">
            <Clock className="w-12 h-12 text-[hsl(var(--text-tertiary))] mx-auto mb-3" />
            <p className="text-[hsl(var(--text-secondary))] text-sm">
              No recent activity
            </p>
            <p className="text-[hsl(var(--text-tertiary))] text-xs mt-1">
              Activity will appear here as you index documents
            </p>
          </div>
        ) : (
          <div className="space-y-4">
            {activities.map((activity) => {
              const ActionIcon = getActionIcon(activity.action);
              const statusColor = getActionColor(activity.status);

              return (
                <div
                  key={activity.id}
                  className="flex items-start gap-3 pb-3 border-b border-[hsl(var(--border-subtle))] last:border-0"
                >
                  <div className={`p-2 rounded-lg bg-[hsl(var(--surface-raised))] ${statusColor}`}>
                    <ActionIcon className="w-4 h-4" />
                  </div>

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-[hsl(var(--text-primary))] capitalize">
                        {activity.action}
                      </span>
                      <span className={`text-xs px-2 py-0.5 rounded-full ${
                        activity.status.toLowerCase() === 'success'
                          ? 'bg-[hsl(var(--success-muted))]/20 text-[hsl(var(--success-fg))]'
                          : activity.status.toLowerCase() === 'error'
                          ? 'bg-[hsl(var(--danger-muted))]/20 text-[hsl(var(--danger-fg))]'
                          : 'bg-[hsl(var(--bg))] text-[hsl(var(--text-secondary))]'
                      }`}>
                        {activity.status}
                      </span>
                    </div>

                    <p
                      className="text-xs text-[hsl(var(--text-secondary))] mt-1 truncate"
                      title={activity.file_path}
                    >
                      {truncatePath(activity.file_path)}
                    </p>

                    {activity.details && (
                      <p className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
                        {activity.details}
                      </p>
                    )}

                    <p className="text-xs text-[hsl(var(--text-tertiary))] mt-1">
                      {getRelativeTime(activity.timestamp)}
                    </p>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default RecentActivity;
