import { type ReactNode } from 'react';

import { motion } from 'framer-motion';

import { fadeInUp } from '@/lib/animations';
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
    up: 'text-success',
    down: 'text-error',
    neutral: 'text-text-secondary',
  };

  return (
    <motion.div variants={fadeInUp} className="flex flex-col items-center space-y-2">
      <div className="text-accent-primary">{icon}</div>
      <div className="text-3xl font-bold text-text-primary tracking-tight tabular-nums">{value}</div>
      <div className="text-sm font-medium text-text-secondary">{label}</div>
      {trend && (
        <div className={cn('text-xs font-medium', trendColors[trendDirection])}>
          {trend}
        </div>
      )}
    </motion.div>
  );
};
