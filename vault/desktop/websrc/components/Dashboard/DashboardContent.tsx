import { DashboardStats } from './DashboardStats';
import { QuickActions } from './QuickActions';
import { RecentActivity } from './RecentActivity';
import { RecentDocuments } from './RecentDocuments';

import type { IndexingStats, IndexingActivity, RecentDocument } from '../../types';

interface DashboardContentProps {
  stats: IndexingStats;
  recentDocuments: RecentDocument[];
  recentActivity: IndexingActivity[];
  onNavigate?: (view: 'search' | 'files' | 'settings') => void;
}

export function DashboardContent({
  stats,
  recentDocuments,
  recentActivity,
  onNavigate,
}: DashboardContentProps) {
  return (
    <>
      <DashboardStats stats={stats} />
      <QuickActions onNavigate={onNavigate} />
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2">
          <RecentDocuments documents={recentDocuments} />
        </div>
        <div>
          <RecentActivity activities={recentActivity} />
        </div>
      </div>
    </>
  );
}
