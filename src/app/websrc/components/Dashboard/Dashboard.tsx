import { FileText, FileUp, Globe, Search } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { useDashboardQuery } from '@/hooks/queries';
import type { RecentDocument } from '@/types';
import { formatBytes, formatRelativeTime } from '@/utils/formatters';

import { DashboardError } from './DashboardError';
import { DashboardSkeleton } from './DashboardSkeleton';

interface DashboardProps {
  onNavigate?: (view: 'search' | 'files' | 'settings') => void;
}

function formatTodayLabel(): string {
  const now = new Date();
  return now.toLocaleDateString(undefined, {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  });
}

interface QuickAction {
  icon: typeof FileUp;
  label: string;
  description: string;
  onClick: () => void;
}

interface StatEntry {
  label: string;
  value: string;
}

export const Dashboard = (_props: DashboardProps) => {
  const { data, isLoading, error } = useDashboardQuery();
  const navigate = useNavigate();

  if (isLoading) {
    return <DashboardSkeleton />;
  }

  if (error) {
    return <DashboardError error={error.message} />;
  }

  const stats: StatEntry[] = [
    {
      label: 'Indexed',
      value: String(data?.stats?.documentCount ?? 0),
    },
    {
      label: 'Storage',
      value: formatBytes(data?.stats?.storageUsed ?? 0),
    },
    {
      label: 'Searches today',
      value: String(data?.stats?.searchCount ?? 0),
    },
    {
      label: 'Last indexed',
      value: formatRelativeTime(data?.stats?.lastIndexed, 'Never'),
    },
  ];

  const quickActions: QuickAction[] = [
    {
      icon: FileUp,
      label: 'Add files',
      description: 'Import local files',
      onClick: () => navigate('/ingest'),
    },
    {
      icon: Globe,
      label: 'Add web page',
      description: 'Import from URL',
      onClick: () => navigate('/ingest'),
    },
    {
      icon: Search,
      label: 'Search',
      description: 'Find in your index',
      onClick: () => navigate('/search'),
    },
  ];

  const recentDocuments = (data?.recentDocuments ?? []).slice(0, 5);

  return (
    <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-[hsl(var(--bg))]">
      <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
        <header className="mb-12">
          <h1 className="font-serif text-3xl font-semibold tracking-[-0.02em] text-[hsl(var(--text-primary))]">
            Dashboard
          </h1>
          <p className="mt-1 text-sm text-[hsl(var(--text-tertiary))]">
            {formatTodayLabel()}
          </p>
        </header>

        <section className="mb-12">
          <div className="grid grid-cols-2 gap-x-12 gap-y-8 sm:grid-cols-4">
            {stats.map((stat) => (
              <div key={stat.label}>
                <div className="text-[11px] font-medium uppercase tracking-[0.04em] text-[hsl(var(--text-tertiary))]">
                  {stat.label}
                </div>
                <div className="mt-1 text-2xl font-semibold tabular-nums text-[hsl(var(--text-primary))]">
                  {stat.value}
                </div>
              </div>
            ))}
          </div>
        </section>

        <section className="mb-12">
          <h2 className="pb-3 text-lg font-medium text-[hsl(var(--text-secondary))]">
            Quick actions
          </h2>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {quickActions.map((action) => {
              const Icon = action.icon;
              return (
                <button
                  key={action.label}
                  type="button"
                  onClick={action.onClick}
                  className="flex w-full items-center gap-4 border-b border-[hsl(var(--border-subtle))] px-2 py-4 text-left transition-colors duration-fast hover:bg-[hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
                >
                  <Icon
                    className="h-[18px] w-[18px] shrink-0 text-[hsl(var(--text-tertiary))]"
                    strokeWidth={1.75}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-[hsl(var(--text-primary))]">
                      {action.label}
                    </div>
                    <div className="text-xs text-[hsl(var(--text-muted))]">
                      {action.description}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        </section>

        <section>
          <h2 className="pb-3 text-lg font-medium text-[hsl(var(--text-secondary))]">
            Recent activity
          </h2>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {recentDocuments.length > 0 ? (
              recentDocuments.map((doc: RecentDocument, index: number) => (
                <div
                  key={doc.id || index}
                  className="flex items-center gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-3 transition-colors duration-fast hover:bg-[hsl(var(--surface))]"
                >
                  <FileText
                    className="h-4 w-4 shrink-0 text-[hsl(var(--text-tertiary))]"
                    strokeWidth={1.75}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-[hsl(var(--text-primary))]">
                      {doc.fileName || doc.filePath || 'Untitled'}
                    </div>
                    <div className="text-xs text-[hsl(var(--text-muted))]">
                      {formatRelativeTime(doc.modifiedAt || doc.indexedAt, '')}
                      {doc.fileType ? ` · ${doc.fileType}` : ''}
                    </div>
                  </div>
                </div>
              ))
            ) : (
              <p className="py-4 text-sm text-[hsl(var(--text-tertiary))]">
                Nothing indexed yet.
              </p>
            )}
          </div>
        </section>
      </div>
    </main>
  );
};

export default Dashboard;
