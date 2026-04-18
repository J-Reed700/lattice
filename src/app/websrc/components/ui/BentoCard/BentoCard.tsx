import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

export interface BentoCardProps {
  children: ReactNode;
  className?: string;
  hover?: boolean;
}

/**
 * BentoCard - A simple card primitive for grid layouts
 *
 * Purpose: Minimal card component for grid-based layouts
 * Philosophy: Composition over configuration - keep it simple
 */
export const BentoCard = ({
  children,
  className,
  hover = true,
}: BentoCardProps) => (
    <div
      className={cn(
        'rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))]',
        'p-6 transition-colors duration-fast',
        hover && 'hover:bg-[hsl(var(--surface-raised))] hover:border-[hsl(var(--border-default))]',
        className
      )}
    >
      {children}
    </div>
  );
