import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * EmptyState
 *
 * One sentence, optionally one action. No icon parade, no accent circle.
 * `icon` is accepted for backwards compatibility and rendered small and
 * muted; prefer omitting it.
 */
export interface EmptyStateProps {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: {
    label: string;
    onClick: () => void;
  };
  className?: string;
}

export const EmptyState = ({ icon, title, description, action, className }: EmptyStateProps) => (
  <div className={cn('flex flex-col items-center justify-center px-6 py-16 text-center', className)}>
    {icon ? (
      <div className="mb-3 flex h-5 w-5 items-center justify-center text-text-muted [&>svg]:h-5 [&>svg]:w-5">
        {icon}
      </div>
    ) : null}
    <p className="text-sm text-text-secondary">{title}</p>
    {description ? <p className="mt-1 max-w-sm text-xs text-text-muted">{description}</p> : null}
    {action ? (
      <button
        type="button"
        onClick={action.onClick}
        className="mt-4 inline-flex h-8 items-center rounded-md border border-border-default bg-surface px-3 text-sm font-medium text-text-primary transition-colors duration-fast hover:bg-surface-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg"
      >
        {action.label}
      </button>
    ) : null}
  </div>
);
