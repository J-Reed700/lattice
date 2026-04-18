import { FileText, HardDrive, Activity, Search } from 'lucide-react';

import Card, { CardHeader, CardTitle, CardContent } from '../ui/Card/Card';

import type { IndexingStats } from '../../types';

/**
 * DashboardStats
 *
 * Purpose: Display key metrics about the Vault workspace
 *
 * Features:
 * - Four statistic cards (documents, chunks, storage estimate, activity)
 * - Icon indicators for each metric
 * - Real-time data updates
 * - Skeleton loading states
 * - Responsive grid layout
 *
 * States: loading, populated
 * Accessibility: WCAG AA, semantic HTML
 */

interface DashboardStatsProps {
  stats: IndexingStats;
  loading?: boolean;
}

export const DashboardStats = ({ stats, loading = false }: DashboardStatsProps) => {
  // Estimate storage based on chunks (rough average of 500 bytes per chunk)
  const estimatedStorageBytes = stats.totalChunks * 500;

  const formatStorage = (bytes: number): string => {
    const kb = bytes / 1024;
    if (kb < 1024) return `${kb.toFixed(1)} KB`;
    const mb = kb / 1024;
    if (mb < 1024) return `${mb.toFixed(1)} MB`;
    const gb = mb / 1024;
    return `${gb.toFixed(2)} GB`;
  };

  const formatNumber = (num: number): string => {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
  };

  const statCards = [
    {
      title: 'Documents Indexed',
      value: formatNumber(stats.indexedDocuments),
      icon: FileText,
      color: 'text-[hsl(var(--accent))]',
      bgColor: 'bg-[hsl(var(--accent-muted))]/20',
      description: 'Total documents in vault',
    },
    {
      title: 'Text Chunks',
      value: formatNumber(stats.totalChunks),
      icon: Activity,
      color: 'text-[hsl(var(--success-fg))]',
      bgColor: 'bg-[hsl(var(--success-muted))]/20',
      description: 'Searchable segments',
    },
    {
      title: 'Estimated Storage',
      value: formatStorage(estimatedStorageBytes),
      icon: HardDrive,
      color: 'text-[hsl(var(--warning-fg))]',
      bgColor: 'bg-[hsl(var(--warning-muted))]/20',
      description: 'Index database size',
    },
    {
      title: 'Search Enabled',
      value: stats.indexedDocuments > 0 ? 'Ready' : 'Empty',
      icon: Search,
      color: 'text-[hsl(var(--danger-fg))]',
      bgColor: 'bg-[hsl(var(--danger-muted))]/20',
      description: 'Vector search status',
    },
  ];

  if (loading) {
    return (
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {[...Array(4)].map((_, i) => (
          <Card key={i} padding="md">
            <div className="animate-pulse space-y-3">
              <div className="h-4 bg-[hsl(var(--surface-raised))] rounded w-1/2" />
              <div className="h-8 bg-[hsl(var(--surface-raised))] rounded w-3/4" />
              <div className="h-3 bg-[hsl(var(--surface-raised))] rounded w-full" />
            </div>
          </Card>
        ))}
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      {statCards.map((stat, index) => {
        const Icon = stat.icon;
        return (
          <Card key={index} padding="md" className="transition-transform hover:scale-105">
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium text-[hsl(var(--text-secondary))]">
                  {stat.title}
                </CardTitle>
                <div className={`p-2 rounded-lg ${stat.bgColor}`}>
                  <Icon className={`w-4 h-4 ${stat.color}`} />
                </div>
              </div>
            </CardHeader>
            <CardContent className="mt-3">
              <div className="text-2xl font-bold text-[hsl(var(--text-primary))]">
                {stat.value}
              </div>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
                {stat.description}
              </p>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
};

export default DashboardStats;
