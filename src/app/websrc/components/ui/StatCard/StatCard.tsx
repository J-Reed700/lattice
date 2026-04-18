import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

export interface StatCardProps {
  icon: ReactNode;
  value: string | number;
  label: string;
  trend?: string;
  trendDirection?: 'up' | 'down' | 'neutral';
}

export const StatCard = ({ icon, value, label, trend, trendDirection = 'neutral' }: StatCardProps) => {
  const trendColors = {
    up: 'text-[hsl(var(--success-fg))]',
    down: 'text-[hsl(var(--danger-fg))]',
    neutral: 'text-[hsl(var(--text-secondary))]',
  };

  return (
    <div className="flex flex-col items-center space-y-2">
      <div className="text-[hsl(var(--accent))]">{icon}</div>
      <div className="text-3xl font-bold text-[hsl(var(--text-primary))] tracking-tight tabular-nums">{value}</div>
      <div className="text-sm font-medium text-[hsl(var(--text-secondary))]">{label}</div>
      {trend && (
        <div className={cn('text-xs font-medium', trendColors[trendDirection])}>
          {trend}
        </div>
      )}
    </div>
  );
};
